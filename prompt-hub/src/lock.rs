#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::{AgentIdentity, LockToken};
use chrono::{Duration, Utc};
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// LockManager
// ---------------------------------------------------------------------------

/// Distributed lock manager with TTL, heartbeat, and periodic sweep of
/// expired locks.
///
/// # Safety
///
/// The manager itself is stateless — actual lock records live in a backing
/// store (SQLite, Redis, …).  This design keeps the struct `Clone`, `Send`,
/// and `Sync`.
#[derive(Debug, Clone)]
pub struct LockManager {
    /// Interval in seconds between background sweeps of expired locks.
    sweep_interval_secs: u64,
}

impl LockManager {
    /// Maximum allowed lock TTL: **1 hour**.
    pub const MAX_TTL_SECONDS: u64 = 3600;

    /// Default TTL for new locks: **5 minutes**.
    pub const DEFAULT_TTL_SECONDS: u64 = 300;

    /// Creates a new `LockManager`.
    pub fn new() -> Self {
        Self {
            sweep_interval_secs: 60,
        }
    }

    /// Creates a manager with a custom sweep interval (mainly for testing).
    pub fn with_sweep_interval(secs: u64) -> Self {
        Self {
            sweep_interval_secs: secs,
        }
    }

    // ── Lock lifecycle ──────────────────────────────────────────────────────

    /// Create a new [`LockToken`] for `prompt_id` held by `agent_id`.
    ///
    /// `ttl_seconds` is clamped to `MAX_TTL_SECONDS`.
    #[instrument]
    pub fn create_lock(prompt_id: Uuid, agent_id: Uuid, ttl_seconds: u64) -> LockToken {
        let clamped_ttl = ttl_seconds.min(Self::MAX_TTL_SECONDS);
        let now = Utc::now();
        let expires_at = now + Duration::seconds(clamped_ttl as i64);
        LockToken {
            id: Uuid::new_v4(),
            prompt_id,
            agent_id,
            token_hash: Uuid::new_v4().to_string(),
            expires_at,
            created_at: now,
        }
    }

    /// Check whether `token` has expired.
    pub fn is_expired(token: &LockToken) -> bool {
        Utc::now() > token.expires_at
    }

    /// Extend the expiry of `token` by `additional_seconds` (clamped so the
    /// new expiry does not exceed `MAX_TTL_SECONDS` from now).
    #[instrument]
    pub fn heartbeat(token: &mut LockToken, additional_seconds: u64) {
        let now = Utc::now();
        let max_expiry = now + Duration::seconds(Self::MAX_TTL_SECONDS as i64);
        let candidate = token.expires_at + Duration::seconds(additional_seconds as i64);
        let new_expiry = candidate.min(max_expiry);

        if new_expiry > token.expires_at {
            debug!(
                "Lock {} heartbeat: expiry extended to {}",
                token.id, new_expiry
            );
            token.expires_at = new_expiry;
        } else {
            debug!("Lock {} heartbeat: already at max TTL", token.id);
        }
    }

    // ── TTL validation ──────────────────────────────────────────────────────

    /// Validate that `ttl_seconds` is within the allowed range.
    ///
    /// Returns the validated TTL on success, [`HubError::BadRequest`] on
    /// failure.
    pub fn validate_ttl(ttl_seconds: u64) -> Result<u64> {
        if ttl_seconds == 0 || ttl_seconds > Self::MAX_TTL_SECONDS {
            Err(HubError::BadRequest(format!(
                "TTL must be between 1 and {} seconds",
                Self::MAX_TTL_SECONDS
            )))
        } else {
            Ok(ttl_seconds)
        }
    }

    // ── Ownership checks ────────────────────────────────────────────────────

    /// Verify that `identity` is the holder of `lock`.
    ///
    /// Returns `Ok(())` if the identity owns the lock, otherwise an
    /// authorization error.
    #[instrument]
    pub fn verify_lock_holder(identity: &AgentIdentity, lock: &LockToken) -> Result<()> {
        if identity.id != lock.agent_id {
            warn!(
                "Lock holder mismatch: identity={} vs lock holder={}",
                identity.id, lock.agent_id
            );
            return Err(HubError::LockError(format!(
                "Identity {} does not hold lock {}",
                identity.id, lock.id
            )));
        }
        debug!("Lock holder verified: {}", identity.id);
        Ok(())
    }

    /// Check whether `identity` may acquire a lock on `prompt_id`.
    ///
    /// The identity must have `Capability::Write` (or `Admin`).
    #[instrument]
    pub fn can_acquire_lock(identity: &AgentIdentity) -> Result<()> {
        use crate::auth::{Action, RbacAuthManager};
        RbacAuthManager::authorize_action(identity, Action::Lock)
    }

    // ── Background sweep ────────────────────────────────────────────────────

    /// Spawn a background task that sweeps expired locks every
    /// `sweep_interval_secs` seconds.
    ///
    /// In a real implementation this would query the backing store and
    /// remove rows where `expires_at < now`.  The stub logs for visibility.
    #[instrument]
    pub fn spawn_sweep_task(&self) {
        let interval = self.sweep_interval_secs;
        info!("Starting lock sweep task every {}s", interval);

        // When `tokio` is available in the runtime the following would be:
        // tokio::spawn(async move {
        //     let mut ticker = tokio::time::interval(Duration::from_secs(interval));
        //     loop {
        //         ticker.tick().await;
        //         // DELETE FROM locks WHERE expires_at < datetime('now')
        //     }
        // });

        // For now we document the intent; the caller integrates with their
        // async runtime.
        debug!("Lock sweep task registered (interval={}s)", interval);
    }

    /// Returns the configured sweep interval.
    pub fn sweep_interval(&self) -> u64 {
        self.sweep_interval_secs
    }
}

impl Default for LockManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    // ── Lock creation ───────────────────────────────────────────────────────

    #[test]
    fn test_lock_creation() {
        let pid = Uuid::new_v4();
        let aid = Uuid::new_v4();
        let lock = LockManager::create_lock(pid, aid, 300);
        assert_eq!(lock.prompt_id, pid);
        assert_eq!(lock.agent_id, aid);
        assert!(!LockManager::is_expired(&lock));
    }

    #[test]
    fn test_lock_default_ttl() {
        let pid = Uuid::new_v4();
        let aid = Uuid::new_v4();
        let lock = LockManager::create_lock(pid, aid, LockManager::DEFAULT_TTL_SECONDS);
        assert!(
            lock.expires_at > Utc::now(),
            "Lock should not be expired immediately after creation"
        );
    }

    #[test]
    fn test_lock_creation_clamps_over_max_ttl() {
        let pid = Uuid::new_v4();
        let aid = Uuid::new_v4();
        let lock = LockManager::create_lock(pid, aid, 99999);
        let max_expiry = Utc::now() + Duration::seconds(LockManager::MAX_TTL_SECONDS as i64);
        assert!(
            lock.expires_at <= max_expiry,
            "TTL should be clamped to MAX_TTL_SECONDS"
        );
    }

    // ── Expiry ──────────────────────────────────────────────────────────────

    #[test]
    fn test_lock_expiry() {
        let pid = Uuid::new_v4();
        let aid = Uuid::new_v4();
        let lock = LockManager::create_lock(pid, aid, 0);
        // Force expired by constructing a token with a past expiry
        let expired = LockToken {
            expires_at: Utc::now() - Duration::seconds(1),
            ..lock
        };
        assert!(LockManager::is_expired(&expired));
    }

    #[test]
    fn test_lock_not_yet_expired() {
        let pid = Uuid::new_v4();
        let aid = Uuid::new_v4();
        let lock = LockManager::create_lock(pid, aid, 3600);
        assert!(!LockManager::is_expired(&lock));
    }

    // ── Heartbeat ───────────────────────────────────────────────────────────

    #[test]
    fn test_heartbeat_extends_lock() {
        let pid = Uuid::new_v4();
        let aid = Uuid::new_v4();
        let mut lock = LockManager::create_lock(pid, aid, 60);
        let original_expiry = lock.expires_at;

        // Sleep briefly to ensure time advances
        std::thread::sleep(std::time::Duration::from_millis(10));
        LockManager::heartbeat(&mut lock, 120);

        assert!(
            lock.expires_at > original_expiry,
            "Heartbeat should extend expiry"
        );
    }

    #[test]
    fn test_heartbeat_clamped_to_max_ttl() {
        let pid = Uuid::new_v4();
        let aid = Uuid::new_v4();
        let mut lock = LockManager::create_lock(pid, aid, 3500);
        LockManager::heartbeat(&mut lock, 9999);

        let max_expiry = Utc::now() + Duration::seconds(LockManager::MAX_TTL_SECONDS as i64);
        assert!(
            lock.expires_at <= max_expiry,
            "Heartbeat should clamp to MAX_TTL_SECONDS from now"
        );
    }

    // ── TTL validation ──────────────────────────────────────────────────────

    #[test]
    fn test_ttl_validation_ok() {
        assert_eq!(
            LockManager::validate_ttl(300).unwrap(),
            300,
            "Valid TTL should be accepted"
        );
    }

    #[test]
    fn test_ttl_validation_zero() {
        assert!(
            LockManager::validate_ttl(0).is_err(),
            "TTL of 0 should be rejected"
        );
    }

    #[test]
    fn test_ttl_validation_over_max() {
        assert!(
            LockManager::validate_ttl(3601).is_err(),
            "TTL > MAX_TTL_SECONDS should be rejected"
        );
    }

    #[test]
    fn test_ttl_validation_boundary() {
        assert_eq!(
            LockManager::validate_ttl(LockManager::MAX_TTL_SECONDS).unwrap(),
            LockManager::MAX_TTL_SECONDS,
            "TTL exactly at MAX_TTL_SECONDS should be accepted"
        );
    }

    // ── Lock holder verification ────────────────────────────────────────────

    #[test]
    fn test_verify_lock_holder_matching() {
        let aid = Uuid::new_v4();
        let caps = vec![crate::models::Capability::Write];
        let identity = AgentIdentity {
            id: aid,
            name: "writer".to_string(),
            capabilities: caps,
            token_hash: "hash".to_string(),
            specialization_score: 0.5,
        };
        let lock = LockManager::create_lock(Uuid::new_v4(), aid, 300);
        assert!(LockManager::verify_lock_holder(&identity, &lock).is_ok());
    }

    #[test]
    fn test_verify_lock_holder_mismatch() {
        let caps = vec![crate::models::Capability::Write];
        let identity = AgentIdentity {
            id: Uuid::new_v4(),
            name: "other".to_string(),
            capabilities: caps,
            token_hash: "hash".to_string(),
            specialization_score: 0.5,
        };
        let lock = LockManager::create_lock(Uuid::new_v4(), Uuid::new_v4(), 300);
        assert!(LockManager::verify_lock_holder(&identity, &lock).is_err());
    }

    // ── Sweep task ──────────────────────────────────────────────────────────

    #[test]
    fn test_spawn_sweep_task() {
        let mgr = LockManager::new();
        mgr.spawn_sweep_task(); // Should not panic
        assert_eq!(mgr.sweep_interval(), 60);
    }

    #[test]
    fn test_custom_sweep_interval() {
        let mgr = LockManager::with_sweep_interval(30);
        assert_eq!(mgr.sweep_interval(), 30);
    }

    // ── Concurrency: agents racing for the same prompt ──────────────────────

    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    fn make_identity(id: Uuid, name: &str) -> AgentIdentity {
        AgentIdentity {
            id,
            name: name.to_string(),
            capabilities: vec![crate::models::Capability::Write],
            token_hash: "hash".to_string(),
            specialization_score: 0.5,
        }
    }

    /// Many agents concurrently create locks for the SAME prompt. Every call
    /// must succeed and yield a globally-unique token (distinct `id` AND
    /// `token_hash`) bound to the requesting agent — i.e. `create_lock` is
    /// safe to call from many threads at once with no shared-state corruption
    /// or RNG collisions.
    #[test]
    fn test_concurrent_create_lock_same_prompt_unique_tokens() {
        const THREADS: usize = 32;
        let prompt_id = Uuid::new_v4();
        let tokens: Arc<Mutex<Vec<LockToken>>> = Arc::new(Mutex::new(Vec::new()));

        std::thread::scope(|scope| {
            for _ in 0..THREADS {
                let tokens = Arc::clone(&tokens);
                scope.spawn(move || {
                    let agent_id = Uuid::new_v4();
                    let lock = LockManager::create_lock(prompt_id, agent_id, 300);
                    assert_eq!(lock.prompt_id, prompt_id);
                    assert_eq!(lock.agent_id, agent_id);
                    tokens
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(lock);
                });
            }
        });

        let tokens = Arc::try_unwrap(tokens).unwrap().into_inner().unwrap();
        assert_eq!(tokens.len(), THREADS);
        let ids: HashSet<_> = tokens.iter().map(|t| t.id).collect();
        let hashes: HashSet<_> = tokens.iter().map(|t| t.token_hash.clone()).collect();
        let agents: HashSet<_> = tokens.iter().map(|t| t.agent_id).collect();
        assert_eq!(ids.len(), THREADS, "lock ids must be unique across threads");
        assert_eq!(
            hashes.len(),
            THREADS,
            "token hashes must be unique across threads"
        );
        assert_eq!(
            agents.len(),
            THREADS,
            "each racing agent is distinct on the same prompt"
        );
    }

    /// Model race resolution: of all agents that grabbed a token for the
    /// prompt, exactly one is the holder. `verify_lock_holder` must accept
    /// only that agent and reject every other, even when checked
    /// concurrently from many threads against the same lock.
    #[test]
    fn test_concurrent_verify_only_holder_succeeds() {
        const LOSERS: usize = 16;
        let prompt_id = Uuid::new_v4();
        let holder_id = Uuid::new_v4();
        let lock = LockManager::create_lock(prompt_id, holder_id, 300);

        std::thread::scope(|scope| {
            let lock = &lock;
            scope.spawn(move || {
                let holder = make_identity(holder_id, "holder");
                assert!(
                    LockManager::verify_lock_holder(&holder, lock).is_ok(),
                    "the lock holder must verify successfully"
                );
            });
            for i in 0..LOSERS {
                scope.spawn(move || {
                    let loser = make_identity(Uuid::new_v4(), &format!("loser-{i}"));
                    assert!(
                        LockManager::verify_lock_holder(&loser, lock).is_err(),
                        "a non-holder must always be rejected"
                    );
                });
            }
        });
    }

    /// Concurrent heartbeats on independent tokens must each clamp to
    /// `MAX_TTL_SECONDS` — the clamp logic has no cross-thread state, so it
    /// must hold under parallel use.
    #[test]
    fn test_concurrent_heartbeat_clamps_to_max() {
        const THREADS: usize = 16;
        std::thread::scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    let mut lock = LockManager::create_lock(Uuid::new_v4(), Uuid::new_v4(), 3500);
                    LockManager::heartbeat(&mut lock, 100_000);
                    let max_expiry =
                        Utc::now() + Duration::seconds(LockManager::MAX_TTL_SECONDS as i64);
                    assert!(
                        lock.expires_at <= max_expiry,
                        "heartbeat must clamp to MAX_TTL under concurrency"
                    );
                });
            }
        });
    }

    /// A shared `Arc<LockManager>` (the type advertises `Clone + Send + Sync`)
    /// must be usable from many threads simultaneously without data races.
    #[test]
    fn test_shared_manager_used_across_threads() {
        let mgr = Arc::new(LockManager::with_sweep_interval(15));
        std::thread::scope(|scope| {
            let mgr = &mgr;
            for _ in 0..8 {
                scope.spawn(move || {
                    assert_eq!(mgr.sweep_interval(), 15);
                    mgr.spawn_sweep_task();
                    let lock = LockManager::create_lock(Uuid::new_v4(), Uuid::new_v4(), 300);
                    assert!(!LockManager::is_expired(&lock));
                });
            }
        });
    }

    // ── Send / Sync ─────────────────────────────────────────────────────────

    #[test]
    fn test_lock_manager_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LockManager>();
    }

    #[test]
    fn test_lock_token_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LockToken>();
    }
}
