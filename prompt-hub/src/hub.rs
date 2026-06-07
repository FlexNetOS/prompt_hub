#![forbid(unsafe_code)]

use crate::auth::{Action, RbacAuthManager};
use crate::config::HubConfig;
use crate::error::{HubError, Result};
use crate::hooks::{HookRegistry, JunieHook};
use crate::metrics::MetricsCollector;
use crate::models::*;
use crate::sanitize::{PromptSanitizer, SanitizationResult};
use crate::search::{FastEngine, HybridEngine, SearchEngine, SmartEngine};
use crate::storage::{Storage, StorageConfig};
use crate::sync::{SyncEvent, SyncManager};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::hash::DefaultHasher;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, instrument, warn};
use uuid::Uuid;

/// Lock manager for prompt editing coordination
pub mod lock {
    use super::*;

    /// Token representing an acquired lock on a prompt
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct LockToken {
        pub prompt_id: Uuid,
        pub agent_id: AgentId,
        pub expires_at: DateTime<Utc>,
        pub token: String,
    }

    /// Lock manager for distributed prompt locking
    #[derive(Debug, Clone, Default)]
    pub struct LockManager {
        #[allow(dead_code)]
        locks: Arc<std::sync::Mutex<Vec<LockToken>>>,
    }

    impl LockManager {
        /// Create a new lock manager
        pub fn new() -> Self {
            Self {
                locks: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        /// Create a lock token for a prompt
        pub fn create_lock(prompt_id: Uuid, agent_id: AgentId, ttl_secs: u64) -> LockToken {
            LockToken {
                prompt_id,
                agent_id,
                expires_at: Utc::now() + chrono::Duration::seconds(ttl_secs as i64),
                token: Uuid::new_v4().to_string(),
            }
        }

        /// Check if a lock token has expired
        pub fn is_expired(token: &LockToken) -> bool {
            Utc::now() > token.expires_at
        }
    }
}

pub use lock::{LockManager, LockToken};

/// Type alias for agent identifiers.
pub type AgentId = Uuid;

/// Compute a tamper-evidence hash over a before→after audit transition.
///
/// Produces a stable hex digest of the concatenated before/after JSON, used to
/// populate [`AuditEntry::diff_hash`].
fn diff_hash(before: Option<&str>, after: Option<&str>) -> String {
    pub use std::hash::{Hash, Hasher};
    let mut hasher: DefaultHasher = DefaultHasher::new();
    before.unwrap_or("").hash(&mut hasher);
    after.unwrap_or("").hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

// ---------------------------------------------------------------------------
// Core PromptHub engine — Send + Sync + 'static
// ---------------------------------------------------------------------------

/// Core PromptHub engine orchestrating storage, search, auth, sanitization,
/// locking, metrics, and sync.
///
/// The engine is `Send + Sync` so it can be shared across axum handlers
/// via an `Arc<PromptHub>`.
#[derive(Debug)]
pub struct PromptHub {
    storage: Arc<Storage>,
    search_engine: Arc<HybridEngine>,
    sanitizer: PromptSanitizer,
    auth: RbacAuthManager,
    lock_manager: LockManager,
    metrics: Arc<MetricsCollector>,
    sync: SyncManager,
    hooks: HookRegistry,
}

impl PromptHub {
    /// Create a new PromptHub instance backed by SQLite storage and a hybrid
    /// search engine.
    #[instrument]
    pub async fn new(db_path: &Path, config: HubConfig) -> Result<Self> {
        let storage_config = StorageConfig {
            db_path: db_path.to_string_lossy().to_string(),
            max_connections: config.max_pool_size,
            wal_mode: true,
            foreign_keys: true,
        };

        let storage = Arc::new(Storage::new(storage_config).await?);
        let fast = Arc::new(FastEngine::new(storage.clone()));
        let smart = Arc::new(SmartEngine::new(
            config.embedding_model,
            storage.clone(),
            config.embedding_dimension,
        ));
        let hybrid = Arc::new(HybridEngine::new(fast, smart));

        info!("PromptHub initialized at {:?}", db_path);

        let metrics = Arc::new(MetricsCollector::default());
        let mut hub = Self {
            storage,
            search_engine: hybrid,
            sanitizer: PromptSanitizer::default(),
            auth: RbacAuthManager::new(),
            lock_manager: LockManager::new(),
            metrics: metrics.clone(),
            sync: SyncManager::new(),
            hooks: HookRegistry::new(),
        };

        // Register default hooks
        hub.hooks.register(Box::new(JunieHook));

        Ok(hub)
    }

    // ── Accessors for server layer ──────────────────────────────────────

    /// Return a cloneable handle to the storage layer.
    pub fn storage(&self) -> Arc<Storage> {
        Arc::clone(&self.storage)
    }

    /// Return a cloneable handle to the metrics collector.
    pub fn metrics(&self) -> Arc<MetricsCollector> {
        Arc::clone(&self.metrics)
    }

    // ── Prompt CRUD ───────────────────────────────────────────────────────

    /// Register a new prompt after sanitization and RBAC checks.
    #[instrument(skip(self, prompt))]
    pub async fn register(&self, prompt: Prompt, identity: &AgentIdentity) -> Result<Uuid> {
        RbacAuthManager::authorize_action(identity, Action::Write)?;

        // Run sanitizer
        match self
            .sanitizer
            .sanitize(&prompt.system_prompt, &prompt.user_template)?
        {
            SanitizationResult::Clean | SanitizationResult::Suspicious(_) => {}
            SanitizationResult::Blocked(issues) => {
                self.metrics.record_sanitization_blocked();
                let summary = issues
                    .iter()
                    .map(|i| format!("[{}] {}", i.category, i.description))
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(HubError::SanitizationError(summary));
            }
        }

        self.storage.insert_prompt(&prompt).await?;
        self.search_engine.index(&prompt).await?;
        self.metrics.record_request();

        let after_json = serde_json::to_string(&prompt).unwrap_or_default();
        self.storage
            .log_audit(&AuditEntry {
                id: 0,
                timestamp: Utc::now(),
                agent_id: identity.id,
                action: format!("{:?}", AuditAction::Created),
                prompt_id: Some(prompt.id),
                diff_hash: diff_hash(None, Some(&after_json)),
                before_json: None,
                after_json: Some(after_json),
                ip_address: None,
            })
            .await?;

        // Sync broadcast is best-effort: an error just means no subscribers
        // are listening, which must not fail the registration.
        let _ = self.sync.broadcast(SyncEvent::PromptAdded {
            prompt_id: prompt.id,
        });

        info!(
            "Registered prompt {} by {} (agent {})",
            prompt.id, identity.name, identity.id
        );
        Ok(prompt.id)
    }

    /// Retrieve a single prompt by role and intent.
    #[instrument(skip(self))]
    pub async fn get(
        &self,
        role: Role,
        intent: &str,
        identity: &AgentIdentity,
    ) -> Result<Option<Prompt>> {
        RbacAuthManager::authorize_action(identity, Action::Read)?;
        self.metrics.record_request();

        // Simplified: use search engine to find best matching prompt.
        let filters = SearchFilters {
            role: Some(role),
            ..SearchFilters::default()
        };
        let results = self
            .search_engine
            .search(intent, &filters, &Pagination::default())
            .await?;
        Ok(results.items.into_iter().next().map(|sp| sp.prompt))
    }

    /// Search prompts using the configured search engine.
    #[instrument(skip(self))]
    pub async fn search(
        &self,
        query: &str,
        _mode: SearchMode,
        filters: SearchFilters,
        pagination: Pagination,
    ) -> Result<Paginated<ScoredPrompt>> {
        self.search_engine
            .search(query, &filters, &pagination)
            .await
    }

    /// List all prompts with pagination.
    #[instrument(skip(self))]
    pub async fn list(&self, pagination: Pagination) -> Result<Paginated<Prompt>> {
        let offset = (pagination.page.saturating_sub(1)) * pagination.per_page;
        let items = self
            .storage
            .list_prompts(None, None, pagination.per_page, offset)
            .await?;
        let total = self.storage.count_prompts(None, None).await? as usize;
        Ok(Paginated {
            items,
            total,
            page: pagination.page,
            per_page: pagination.per_page,
        })
    }

    // ── Lock management ───────────────────────────────────────────────────

    /// Acquire an edit lock on a prompt.
    #[instrument(skip(self))]
    pub async fn lock(
        &self,
        id: Uuid,
        agent: &AgentIdentity,
        ttl: std::time::Duration,
    ) -> Result<LockToken> {
        RbacAuthManager::authorize_action(agent, Action::Lock)?;
        let token = LockManager::create_lock(id, agent.id, ttl.as_secs());
        self.metrics.record_lock_acquired();
        let after_json = token.token.clone();
        self.storage
            .log_audit(&AuditEntry {
                id: 0,
                timestamp: Utc::now(),
                agent_id: agent.id,
                action: format!("{:?}", AuditAction::Locked),
                prompt_id: Some(id),
                diff_hash: diff_hash(None, Some(&after_json)),
                before_json: None,
                after_json: Some(after_json),
                ip_address: None,
            })
            .await?;
        info!("Lock acquired for prompt {} by agent {}", id, agent.id);
        Ok(token)
    }

    /// Release a previously acquired lock.
    #[instrument(skip(self))]
    pub async fn unlock(&self, token: LockToken) -> Result<()> {
        if LockManager::is_expired(&token) {
            warn!(
                "Attempted to release expired lock on prompt {} by agent {}",
                token.prompt_id, token.agent_id
            );
            return Err(HubError::LockError(format!(
                "Lock expired for prompt {} held by agent {}",
                token.prompt_id, token.agent_id
            )));
        }
        self.metrics.record_lock_released();
        let before_json = token.token.clone();
        self.storage
            .log_audit(&AuditEntry {
                id: 0,
                timestamp: Utc::now(),
                agent_id: token.agent_id,
                action: format!("{:?}", AuditAction::Unlocked),
                prompt_id: Some(token.prompt_id),
                diff_hash: diff_hash(Some(&before_json), None),
                before_json: Some(before_json),
                after_json: None,
                ip_address: None,
            })
            .await?;
        info!("Lock released for prompt {}", token.prompt_id);
        Ok(())
    }

    // ── Audit & ownership ─────────────────────────────────────────────────

    /// Get the audit trail for a prompt.
    #[instrument(skip(self))]
    pub async fn audit_trail(
        &self,
        id: Uuid,
        pagination: Pagination,
    ) -> Result<Paginated<AuditEntry>> {
        self.storage
            .fetch_audit_trail(id, pagination.page, pagination.per_page)
            .await
    }

    /// Transfer prompt ownership between agents (admin only).
    #[instrument(skip(self))]
    pub async fn transfer_ownership(
        &self,
        id: Uuid,
        _from: &AgentIdentity,
        to: &AgentIdentity,
        admin: &AgentIdentity,
    ) -> Result<Prompt> {
        RbacAuthManager::authorize_action(admin, Action::Admin)?;
        let before = self.storage.get_prompt(id).await?;
        self.storage.transfer_prompt_ownership(id, to.id).await?;
        self.metrics.record_request();
        let prompt = self
            .storage
            .get_prompt(id)
            .await?
            .ok_or(HubError::NotFound(id.to_string()))?;
        let before_json = before
            .as_ref()
            .map(|b| serde_json::to_string(b).unwrap_or_default());
        let after_json = serde_json::to_string(&prompt).unwrap_or_default();
        self.storage
            .log_audit(&AuditEntry {
                id: 0,
                timestamp: Utc::now(),
                agent_id: admin.id,
                action: format!("{:?}", AuditAction::Created),
                prompt_id: Some(id),
                diff_hash: diff_hash(before_json.as_deref(), Some(&after_json)),
                before_json,
                after_json: Some(after_json),
                ip_address: None,
            })
            .await?;
        info!("Transferred ownership of prompt {} to agent {}", id, to.id);
        Ok(prompt)
    }

    // ── Vibe Coding ───────────────────────────────────────────────────────

    /// Natural-language request → deliverable (Vibe Coding).
    #[instrument(skip(self))]
    pub async fn vibe_code(
        &self,
        request: &str,
        input: UserInput,
        level: SkillLevel,
    ) -> Result<VibeResult> {
        use crate::vibe::VibeEngine;
        let engine = VibeEngine::default();
        let result = engine.vibe_code(request, input, level).await?;
        info!(
            "Vibe coding completed with confidence {}",
            result.confidence
        );
        Ok(result)
    }

    // ── Context gathering ─────────────────────────────────────────────────

    /// Gather project context from the filesystem.
    #[instrument(skip(self))]
    pub async fn gather_context(&self, project_path: &Path) -> Result<ProjectContext> {
        use crate::context_gatherer::ContextGatherer;
        let ctx = ContextGatherer::gather(project_path).await?;
        info!("Gathered context for {}", ctx.project_path);
        Ok(ctx)
    }

    // ── Cost estimation ───────────────────────────────────────────────────

    /// Estimate the cost of an intent in a given project context.
    #[instrument(skip(self))]
    pub async fn estimate_cost(
        &self,
        intent: &Intent,
        context: &ProjectContext,
    ) -> Result<CostEstimate> {
        use crate::cost::CostEstimator;
        let estimator = CostEstimator;
        let estimate = estimator.estimate(intent, context).await?;
        info!(
            "Cost estimate: ${:.4} ({} input / {} output tokens)",
            estimate.estimated_cost_usd, estimate.tokens_input, estimate.tokens_output
        );
        Ok(estimate)
    }

    // ── Privacy scanning ──────────────────────────────────────────────────

    /// Scan user input for privacy issues.
    #[instrument(skip(self))]
    pub async fn scan_privacy(&self, input: &UserInput) -> Result<PrivacyReport> {
        use crate::privacy::PrivacyScanner;
        let scanner = PrivacyScanner::default();
        let report = scanner.scan(input).await?;
        info!("Privacy scan completed: {:?} risk level", report.risk_level);
        Ok(report)
    }

    // ── Confidence scoring ────────────────────────────────────────────────

    /// Score confidence for an intent against project context.
    #[instrument(skip(self))]
    pub async fn score_confidence(
        &self,
        intent: &Intent,
        context: &ProjectContext,
    ) -> Result<ConfidenceScore> {
        use crate::confidence::ConfidenceScorer;
        let scorer = ConfidenceScorer::from_intent(intent, context);
        let score = scorer.score();
        info!("Confidence score: {:.2}", score.score);
        Ok(score)
    }

    // ── Graceful shutdown helper ──────────────────────────────────────────

    /// Gracefully shut down the hub: close storage, drain metrics.
    #[instrument(skip(self))]
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down PromptHub storage...");
        self.storage.optimize_on_close().await?;
        info!("PromptHub shutdown complete");
        Ok(())
    }

    // ── Prompt lifecycle ──────────────────────────────────────────────────

    /// Update an existing prompt.
    #[instrument(skip(self))]
    pub async fn update(
        &self,
        id: Uuid,
        patch: PromptPatch,
        identity: &AgentIdentity,
    ) -> Result<Prompt> {
        RbacAuthManager::authorize_action(identity, Action::Write)?;
        let before = self.storage.get_prompt(id).await?;
        self.storage.update_prompt(id, &patch).await?;
        self.metrics.record_request();
        let updated = self
            .storage
            .get_prompt(id)
            .await?
            .ok_or(HubError::NotFound(id.to_string()))?;
        let before_json = before
            .as_ref()
            .map(|b| serde_json::to_string(b).unwrap_or_default());
        let after_json = serde_json::to_string(&updated).unwrap_or_default();
        self.storage
            .log_audit(&AuditEntry {
                id: 0,
                timestamp: Utc::now(),
                agent_id: identity.id,
                action: format!("{:?}", AuditAction::Updated),
                prompt_id: Some(id),
                diff_hash: diff_hash(before_json.as_deref(), Some(&after_json)),
                before_json,
                after_json: Some(after_json),
                ip_address: None,
            })
            .await?;
        info!("Updated prompt {}", id);
        Ok(updated)
    }

    /// Rollback a prompt to a previous version.
    #[instrument(skip(self))]
    pub async fn rollback(
        &self,
        id: Uuid,
        to_version: &str,
        identity: &AgentIdentity,
    ) -> Result<Prompt> {
        RbacAuthManager::authorize_action(identity, Action::Write)?;
        let before = self.storage.get_prompt(id).await?;
        self.storage.rollback_prompt(id, to_version).await?;
        self.metrics.record_request();
        let rolled = self
            .storage
            .get_prompt(id)
            .await?
            .ok_or(HubError::NotFound(id.to_string()))?;
        let before_json = before
            .as_ref()
            .map(|b| serde_json::to_string(b).unwrap_or_default());
        let after_json = serde_json::to_string(&rolled).unwrap_or_default();
        self.storage
            .log_audit(&AuditEntry {
                id: 0,
                timestamp: Utc::now(),
                agent_id: identity.id,
                action: format!("{:?}", AuditAction::RolledBack),
                prompt_id: Some(id),
                diff_hash: diff_hash(before_json.as_deref(), Some(&after_json)),
                before_json,
                after_json: Some(after_json),
                ip_address: None,
            })
            .await?;
        info!("Rolled back prompt {} to version {}", id, to_version);
        Ok(rolled)
    }

    /// Evolve a prompt using the specified strategy.
    #[instrument(skip(self))]
    pub async fn evolve_prompt(
        &self,
        id: Uuid,
        strategy: EvolutionStrategy,
        identity: &AgentIdentity,
    ) -> Result<Prompt> {
        RbacAuthManager::authorize_action(identity, Action::Write)?;
        use crate::evolution::EvolutionEngine;
        let base = self
            .storage
            .get_prompt(id)
            .await?
            .ok_or(HubError::NotFound(id.to_string()))?;
        let evolved = match strategy {
            EvolutionStrategy::Mutate => EvolutionEngine::mutate(&base, 0.5)?,
            EvolutionStrategy::Crossover => {
                let candidates = self.storage.list_prompts(None, None, 10, 0).await?;
                if candidates.is_empty() {
                    return Err(HubError::Internal("No crossover candidates".into()));
                }
                EvolutionEngine::crossover(&base, &candidates[0])?
            }
            _ => EvolutionEngine::mutate(&base, 0.3)?,
        };
        self.storage.insert_prompt(&evolved).await?;
        self.search_engine.index(&evolved).await?;
        let before_json = serde_json::to_string(&base).unwrap_or_default();
        let after_json = serde_json::to_string(&evolved).unwrap_or_default();
        self.storage
            .log_audit(&AuditEntry {
                id: 0,
                timestamp: Utc::now(),
                agent_id: identity.id,
                action: format!("{:?}", AuditAction::Evolved),
                prompt_id: Some(id),
                diff_hash: diff_hash(Some(&before_json), Some(&after_json)),
                before_json: Some(before_json),
                after_json: Some(after_json),
                ip_address: None,
            })
            .await?;
        info!("Evolved prompt {} into new prompt {}", id, evolved.id);
        Ok(evolved)
    }

    /// Execute the fallback chain for an intent.
    #[instrument(skip(self))]
    pub async fn fallback_chain(
        &self,
        intent: &Intent,
        context: &ProjectContext,
    ) -> Result<Artifact> {
        use crate::fallback::FallbackChain;
        let chain = FallbackChain::default();
        let artifact = chain.execute(intent, context).await?;
        info!(
            "Fallback chain produced artifact for intent {:?}",
            intent.task_type
        );
        Ok(artifact)
    }

    /// Learn from user feedback to improve future results.
    #[instrument(skip(self))]
    pub async fn learn_from_feedback(
        &self,
        correction: &str,
        intent: &Intent,
        agent_id: Uuid,
    ) -> Result<()> {
        use crate::learn::LearningEngine;
        use crate::models::UserCorrection;
        use chrono::Utc;
        let mut engine = LearningEngine::default();
        let correction = UserCorrection {
            original_intent: intent.raw_text.clone(),
            corrected_output: String::new(),
            feedback: correction.to_string(),
            agent_id,
            timestamp: Utc::now(),
        };
        engine.learn_from_feedback(correction).await?;
        info!("Learned from feedback by agent {}", agent_id);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Send + Sync via field types (Arc<Storage>, Arc<dyn SearchEngine>, ...)
// ---------------------------------------------------------------------------

// All constituent types are Send + Sync, so PromptHub is naturally Send + Sync.
// The explicit impl blocks below document this contract:

// Compile-time Send + Sync assertion — safe replacement for `unsafe impl`.
#[allow(dead_code)]
fn _assert_prompt_hub_send_sync()
where
    PromptHub: Send + Sync + 'static,
{
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    fn test_config() -> HubConfig {
        HubConfig {
            max_pool_size: 2,
            default_page_size: 10,
            max_page_size: 100,
            config_dir: None,
            auto_migrate: true,
            default_search_limit: 10,
            max_search_limit: 100,
            embedding_model: "test-model".to_string(),
            embedding_dimension: 384,
        }
    }

    fn test_agent() -> AgentIdentity {
        AgentIdentity {
            id: Uuid::new_v4(),
            name: "test-agent".to_string(),
            capabilities: vec![Capability::Read, Capability::Write],
            token_hash: "abc123".to_string(),
            specialization_score: 0.8,
        }
    }

    fn test_prompt() -> Prompt {
        Prompt {
            id: Uuid::new_v4(),
            name: "test-prompt".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: "You are a helpful assistant.".to_string(),
            user_template: "Hello, {{name}}!".to_string(),
            required_vars: vec!["name".to_string()],
            domain: Domain::General,
            tags: vec!["test".to_string()],
            target_roles: vec![Role::Developer],
            metadata: PromptMeta::default(),
            metrics: PromptMetrics::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            author: AgentIdentity::default(),
            deleted_at: None,
            generation_params: None,
            locale: None,
            multimodal: None,
        }
    }

    #[tokio::test]
    async fn test_hub_new() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config()).await;
        assert!(hub.is_ok());
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();
        let agent = test_agent();
        let prompt = test_prompt();
        let id = prompt.id;

        let result = hub.register(prompt.clone(), &agent).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), id);

        let fetched = hub.get(Role::Developer, "greet", &agent).await;
        assert!(fetched.is_ok());
    }

    #[tokio::test]
    async fn test_lock_unlock() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();
        let agent = test_agent();
        let prompt_id = Uuid::new_v4();

        let token = hub.lock(prompt_id, &agent, Duration::from_secs(60)).await;
        assert!(token.is_ok());

        let unlock_result = hub.unlock(token.unwrap()).await;
        assert!(unlock_result.is_ok());
    }

    #[tokio::test]
    async fn test_lock_expired_unlock_fails() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();
        let agent = test_agent();
        let prompt_id = Uuid::new_v4();

        // Create an already-expired token manually
        let expired_token = LockToken {
            prompt_id,
            agent_id: agent.id,
            expires_at: Utc::now() - chrono::Duration::seconds(1),
            token: "expired".to_string(),
        };

        let result = hub.unlock(expired_token).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_search() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let results = hub
            .search(
                "hello",
                SearchMode::Hybrid,
                SearchFilters::default(),
                Pagination::default(),
            )
            .await;
        assert!(results.is_ok());
        let paginated = results.unwrap();
        assert_eq!(paginated.page, 1);
    }

    #[tokio::test]
    async fn test_vibe_code() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let result = hub
            .vibe_code(
                "Create a greeting page",
                UserInput::default(),
                SkillLevel::Beginner,
            )
            .await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_send_sync() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<PromptHub>();
    }

    #[test]
    fn test_lock_manager_create_and_expire() {
        let prompt_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let token = LockManager::create_lock(prompt_id, agent_id, 3600);

        assert_eq!(token.prompt_id, prompt_id);
        assert_eq!(token.agent_id, agent_id);
        assert!(!LockManager::is_expired(&token));

        // Expired token
        let expired = LockToken {
            prompt_id,
            agent_id,
            expires_at: Utc::now() - chrono::Duration::seconds(1),
            token: "old".to_string(),
        };
        assert!(LockManager::is_expired(&expired));
    }
}
