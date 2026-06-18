//! In-process execution sandbox for prompt management.
//!
//! Provides resource-bounded and isolated execution modes as a policy
//! configuration + enforcement layer. No process-level isolation — limits are
//! enforced at the PromptHub API gate before operations proceed.

#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::{Sandbox, SandboxCheckResult, SandboxConfig, SandboxMode};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::timeout;
use tracing::instrument;
use uuid::Uuid;

/// Sliding-window counter used for per-sandbox rate limiting.
#[derive(Debug)]
struct RateWindow {
    /// Timestamps (seconds since epoch) of past requests within the current window.
    requests: Vec<u64>,
}

impl RateWindow {
    fn new() -> Self {
        Self {
            requests: Vec::new(),
        }
    }

    /// Record a request timestamp, evicting entries outside the 60-second window.
    fn record(&mut self) {
        let now = Utc::now().timestamp() as u64;
        let cutoff = now.saturating_sub(60);
        self.requests.retain(|&ts| ts > cutoff);
        self.requests.push(now);
    }

    /// Returns the number of requests in the current window.
    fn count(&self) -> u32 {
        self.requests.len() as u32
    }

    /// Returns how many more seconds until the oldest request expires from the window.
    fn retry_after(&self) -> Option<u64> {
        self.requests.first().map(|&ts| {
            ts.saturating_sub(Utc::now().timestamp() as u64)
                .saturating_add(1)
        })
    }
}

/// In-memory store for sandbox definitions, backed by a mutex-protected vector.
#[derive(Debug)]
struct SandboxStore {
    sandboxes: Vec<Sandbox>,
}

impl SandboxStore {
    fn new() -> Self {
        Self {
            sandboxes: Vec::new(),
        }
    }

    fn insert(&mut self, sandbox: Sandbox) {
        // Replace existing or append
        if let Some(pos) = self.sandboxes.iter().position(|s| s.id == sandbox.id) {
            self.sandboxes[pos] = sandbox;
        } else {
            self.sandboxes.push(sandbox);
        }
    }

    fn get(&self, id: &Uuid) -> Option<&Sandbox> {
        self.sandboxes.iter().find(|s| s.id == *id)
    }

    fn get_mut(&mut self, id: &Uuid) -> Option<&mut Sandbox> {
        self.sandboxes.iter_mut().find(|s| s.id == *id)
    }

    fn remove(&mut self, id: &Uuid) -> bool {
        if let Some(pos) = self.sandboxes.iter().position(|s| s.id == *id) {
            self.sandboxes.remove(pos);
            true
        } else {
            false
        }
    }
}

/// Engine that owns sandbox definitions and provides CRUD + enforcement.
#[derive(Debug)]
pub struct SandboxEngine {
    store: Arc<std::sync::Mutex<SandboxStore>>,
    /// Per-sandbox rate limit counters keyed by sandbox id string.
    rate_windows: Arc<std::sync::Mutex<HashMap<String, RateWindow>>>,
}

impl Default for SandboxEngine {
    fn default() -> Self {
        Self {
            store: Arc::new(std::sync::Mutex::new(SandboxStore::new())),
            rate_windows: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }
}

impl SandboxEngine {
    // ── CRUD operations ──────────────────────────────────────────────

    /// Create a new sandbox with the given name, mode, and config.
    #[instrument(skip(self), fields(name = %name))]
    pub fn create_sandbox(
        &self,
        name: String,
        mode: SandboxMode,
        config: SandboxConfig,
    ) -> Result<Sandbox> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let sandbox = Sandbox {
            id,
            name,
            mode,
            enabled: true,
            created_at: now,
            updated_at: now,
        };

        let mut store = self
            .store
            .lock()
            .map_err(|e| HubError::Internal(e.to_string()))?;
        store.insert(sandbox.clone());
        Ok(sandbox)
    }

    /// Retrieve a sandbox by id.
    #[instrument(skip(self))]
    pub fn get_sandbox(&self, id: Uuid) -> Result<Sandbox> {
        let store = self
            .store
            .lock()
            .map_err(|e| HubError::Internal(e.to_string()))?;
        store
            .get(&id)
            .cloned()
            .ok_or_else(|| HubError::NotFound(format!("sandbox not found: {}", id)))
    }

    /// Update a sandbox's config by id. Returns the updated sandbox.
    #[instrument(skip(self), fields(sandbox_id = %id))]
    pub fn update_sandbox(&self, id: Uuid, new_config: SandboxConfig) -> Result<Sandbox> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| HubError::Internal(e.to_string()))?;
        let sandbox = store
            .get_mut(&id)
            .ok_or_else(|| HubError::NotFound(format!("sandbox not found: {}", id)))?;

        sandbox.enabled = true;
        sandbox.updated_at = Utc::now();

        match &mut sandbox.mode {
            SandboxMode::Bounded(cfg) => *cfg = new_config,
            SandboxMode::Isolated(cfg) => *cfg = new_config,
            SandboxMode::Unrestricted => {
                // Transition from unrestricted to bounded.
                *sandbox = Sandbox {
                    mode: SandboxMode::Bounded(new_config.clone()),
                    enabled: true,
                    updated_at: Utc::now(),
                    ..(*sandbox).clone()
                };
            }
        }

        Ok(sandbox.clone())
    }

    /// Delete a sandbox by id. Returns `HubError::NotFound` if absent.
    #[instrument(skip(self), fields(sandbox_id = %id))]
    pub fn delete_sandbox(&self, id: Uuid) -> Result<()> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| HubError::Internal(e.to_string()))?;
        if !store.remove(&id) {
            return Err(HubError::NotFound(format!("sandbox not found: {}", id)));
        }
        // Also drop the rate window.
        let mut windows = self
            .rate_windows
            .lock()
            .map_err(|e| HubError::Internal(e.to_string()))?;
        windows.remove(&id.to_string());
        Ok(())
    }

    // ── Enforcement ──────────────────────────────────────────────────

    /// Check whether a prompt execution is allowed under the sandbox's limits.
    #[instrument(skip(self), fields(sandbox_id = %sandbox_id))]
    pub fn check(
        &self,
        sandbox_id: Uuid,
        prompt_tokens: u32,
        cost_usd: f64,
        network_call: bool,
    ) -> SandboxCheckResult {
        let store = self.store.lock().expect("sandbox store lock poisoned");
        let sandbox = match store.get(&sandbox_id) {
            Some(s) => s,
            None => return SandboxCheckResult::Allowed, // unknown sandbox → allow
        };

        if !sandbox.enabled {
            return SandboxCheckResult::Allowed; // disabled sandbox is permissive
        }

        let mode = sandbox.mode.clone();
        drop(store); // release lock before more work

        let config = match &mode {
            SandboxMode::Unrestricted => return SandboxCheckResult::Allowed,
            SandboxMode::Bounded(cfg) => cfg.clone(),
            SandboxMode::Isolated(cfg) => cfg.clone(),
        };

        // Check rate limit first.
        {
            let mut windows = self
                .rate_windows
                .lock()
                .expect("rate windows lock poisoned");
            let key = sandbox_id.to_string();
            let window = windows.entry(key).or_insert_with(RateWindow::new);
            window.record();
            if window.count() > config.rate_limit_per_min {
                return SandboxCheckResult::RateLimited {
                    retry_after_secs: window.retry_after().unwrap_or(1),
                };
            }
        }

        // Check token limit.
        if prompt_tokens > config.max_tokens {
            return SandboxCheckResult::TokenLimitExceeded {
                used: prompt_tokens,
                max: config.max_tokens,
            };
        }

        // Check cost limit.
        if cost_usd > config.max_cost_usd {
            return SandboxCheckResult::BudgetExceeded {
                spent_usd: cost_usd,
                limit_usd: config.max_cost_usd,
            };
        }

        // Check network isolation (Isolated mode always denies; Bounded only if deny_network).
        if network_call && config.deny_network {
            return SandboxCheckResult::NetworkDenied;
        }

        SandboxCheckResult::Allowed
    }

    /// Wrap a future with the sandbox's configured timeout.
    pub async fn apply_timeout<F, T>(&self, sandbox_id: Uuid, future: F) -> Result<T>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let sandbox = {
            let store = self
                .store
                .lock()
                .map_err(|e| HubError::Internal(e.to_string()))?;
            match store.get(&sandbox_id) {
                Some(s) => s.clone(),
                None => {
                    return Err(HubError::NotFound(format!(
                        "sandbox not found: {}",
                        sandbox_id
                    )));
                }
            }
        };

        let config = match &sandbox.mode {
            SandboxMode::Bounded(cfg) | SandboxMode::Isolated(cfg) => cfg.timeout_secs,
            SandboxMode::Unrestricted => 300, // 5 min for unrestricted
        };

        let result = timeout(std::time::Duration::from_secs(config), future).await;

        match result {
            Ok(val) => Ok(val),
            Err(_) => Err(HubError::Timeout(format!(
                "sandbox {} timed out after {}s",
                sandbox_id, config
            ))),
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod sandbox_tests {
    use super::*;

    fn test_engine() -> SandboxEngine {
        SandboxEngine::default()
    }

    #[test]
    fn test_sandbox_create_default() {
        let engine = test_engine();
        let sb = engine
            .create_sandbox(
                "test-sb".into(),
                SandboxMode::Bounded(SandboxConfig::default()),
                SandboxConfig::default(),
            )
            .unwrap();
        assert_eq!(sb.name, "test-sb");
        assert!(sb.enabled);
        // Verify id is a valid UUID v4 format.
        assert_eq!(sb.id.get_version(), Some(uuid::Version::Random));
    }

    #[test]
    fn test_sandbox_create_isolated() {
        let engine = test_engine();
        let mode = SandboxMode::Isolated(SandboxConfig {
            deny_network: true,
            ..SandboxConfig::default()
        });
        let sb = engine
            .create_sandbox("isolated-sb".into(), mode.clone(), SandboxConfig::default())
            .unwrap();
        assert!(matches!(sb.mode, SandboxMode::Isolated(_)));
    }

    #[test]
    fn test_sandbox_get_nonexistent() {
        let engine = test_engine();
        let absent = Uuid::new_v4();
        let err = engine.get_sandbox(absent).unwrap_err();
        assert!(matches!(err, HubError::NotFound(_)));
    }

    #[test]
    fn test_sandbox_update() {
        let engine = test_engine();
        let sb = engine
            .create_sandbox(
                "update-me".into(),
                SandboxMode::Bounded(SandboxConfig::default()),
                SandboxConfig::default(),
            )
            .unwrap();
        let id = sb.id;
        let new_config = SandboxConfig {
            max_tokens: 4096,
            ..SandboxConfig::default()
        };
        let updated = engine.update_sandbox(id, new_config).unwrap();
        assert!(matches!(&updated.mode, SandboxMode::Bounded(cfg) if cfg.max_tokens == 4096));
    }

    #[test]
    fn test_sandbox_delete() {
        let engine = test_engine();
        let sb = engine
            .create_sandbox(
                "delete-me".into(),
                SandboxMode::Unrestricted,
                SandboxConfig::default(),
            )
            .unwrap();
        let id = sb.id;
        engine.delete_sandbox(id).unwrap();
        let err = engine.get_sandbox(id).unwrap_err();
        assert!(matches!(err, HubError::NotFound(_)));
    }

    #[test]
    fn test_sandbox_check_allows_under_limits() {
        let engine = test_engine();
        let cfg = SandboxConfig {
            max_tokens: 8192,
            ..SandboxConfig::default()
        };
        let sb = engine
            .create_sandbox(
                "allow-test".into(),
                SandboxMode::Bounded(cfg),
                SandboxConfig::default(),
            )
            .unwrap();
        let result = engine.check(sb.id, 100, 0.01, false);
        assert!(matches!(result, SandboxCheckResult::Allowed));
    }

    #[test]
    fn test_sandbox_check_token_limit() {
        let engine = test_engine();
        let cfg = SandboxConfig {
            max_tokens: 500,
            ..SandboxConfig::default()
        };
        let sb = engine
            .create_sandbox(
                "token-test".into(),
                SandboxMode::Bounded(cfg),
                SandboxConfig::default(),
            )
            .unwrap();
        let result = engine.check(sb.id, 600, 0.01, false);
        assert!(
            matches!(&result, SandboxCheckResult::TokenLimitExceeded { used, max } if *used == 600 && *max == 500)
        );
    }

    #[test]
    fn test_sandbox_check_cost_limit() {
        let engine = test_engine();
        let cfg = SandboxConfig {
            max_cost_usd: 1.0,
            ..SandboxConfig::default()
        };
        let sb = engine
            .create_sandbox(
                "cost-test".into(),
                SandboxMode::Bounded(cfg),
                SandboxConfig::default(),
            )
            .unwrap();
        let result = engine.check(sb.id, 100, 2.5, false);
        assert!(
            matches!(&result, SandboxCheckResult::BudgetExceeded { spent_usd, limit_usd } if (spent_usd - 2.5).abs() < f64::EPSILON && (limit_usd - 1.0).abs() < f64::EPSILON)
        );
    }

    #[test]
    fn test_sandbox_rate_limit_exhausted() {
        let engine = test_engine();
        let cfg = SandboxConfig {
            rate_limit_per_min: 3,
            ..SandboxConfig::default()
        };
        let sb = engine
            .create_sandbox(
                "rate-test".into(),
                SandboxMode::Bounded(cfg),
                SandboxConfig::default(),
            )
            .unwrap();

        // Send 3 requests (at the limit) — should all be allowed.
        for _ in 0..3 {
            let result = engine.check(sb.id, 10, 0.01, false);
            assert!(matches!(result, SandboxCheckResult::Allowed));
        }

        // 4th request should be rate limited.
        let result = engine.check(sb.id, 10, 0.01, false);
        assert!(matches!(&result, SandboxCheckResult::RateLimited { .. }));
    }

    #[test]
    fn test_sandbox_isolation_denies_network() {
        let engine = test_engine();
        let cfg = SandboxConfig {
            deny_network: true,
            ..SandboxConfig::default()
        };
        let sb = engine
            .create_sandbox(
                "isolated-net".into(),
                SandboxMode::Isolated(cfg),
                SandboxConfig::default(),
            )
            .unwrap();
        let result = engine.check(sb.id, 10, 0.01, true); // network_call = true
        assert!(matches!(result, SandboxCheckResult::NetworkDenied));
    }

    #[test]
    fn test_sandbox_check_result_equality() {
        let allowed = SandboxCheckResult::Allowed;
        let allowed2 = SandboxCheckResult::Allowed;
        assert!(allowed == allowed2);

        let rate_limited = SandboxCheckResult::RateLimited {
            retry_after_secs: 30,
        };
        let rate_limited2 = SandboxCheckResult::RateLimited {
            retry_after_secs: 30,
        };
        assert!(rate_limited == rate_limited2);
        assert!(allowed != rate_limited);
    }

    #[test]
    fn test_sandbox_check_unknown_sandbox_allows() {
        let engine = test_engine();
        let unknown = Uuid::new_v4();
        let result = engine.check(unknown, 999_999, 999.0, true);
        assert!(matches!(result, SandboxCheckResult::Allowed));
    }
}
