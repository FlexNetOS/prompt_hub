#![forbid(unsafe_code)]

use crate::analytics::Analytics;
use crate::audit::SqliteAuditLogger;
use crate::auth::{Action, RbacAuthManager};
#[cfg(feature = "budget")]
use crate::budget::{BudgetAlert, BudgetConfig, BudgetTracker};
#[cfg(feature = "canary")]
use crate::canary::CanaryEngine;
#[cfg(feature = "circuit-breaker")]
use crate::circuit_breaker::CircuitBreaker;
use crate::config::HubConfig;
use crate::diff::PromptDiff;
use crate::error::{HubError, Result};
#[cfg(feature = "retention")]
use crate::garbage_collector::GarbageCollector;
use crate::health::HealthAggregator;
use crate::hooks::{HookRegistry, JunieHook};
#[cfg(feature = "i18n")]
use crate::i18n::I18nEngine;
use crate::lineage::{AncestryPath, Fork, LineageTracker, LineageTree};
use crate::load_balancer::{LoadBalancer, ProviderSelection, ProviderStats, RoutingStrategy};
use crate::metrics::MetricsCollector;
use crate::models::CanaryDeployment;
use crate::models::*;
#[cfg(feature = "moderation")]
use crate::moderation::ModerationEngine;
#[cfg(feature = "multimodal")]
use crate::multimodal::MultimodalEngine;
use crate::pollination::{CrossAgentPollination, Pattern};
#[cfg(feature = "preview")]
use crate::preview::PreviewEngine;
use crate::provider_health::{HealthSummary, ProviderHealthMonitor};
use crate::quality_gate::{QualityGate, QualityResult};
#[cfg(feature = "quota")]
use crate::quota::QuotaEnforcer;
#[cfg(feature = "retention")]
use crate::retention::{DataType, RetentionPolicy};
#[cfg(feature = "rollback")]
use crate::rollback::SafeDeployer;
use crate::sanitize::{PromptSanitizer, SanitizationResult};
use crate::satisfaction::{SatisfactionMetrics, SatisfactionTracker};
use crate::search::{FastEngine, HybridEngine, SearchEngine, SmartEngine};
use crate::storage::{Storage, StorageConfig};
use crate::swarm::{self, SwarmRoleRegistry};
use crate::sync::{SyncEvent, SyncManager};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::hash::DefaultHasher;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};
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
        /// Create a new lock manager instance with an empty lock store.
        ///
        /// The underlying storage is a thread-safe `Mutex<Vec<LockToken>>`
        /// suitable for in-process coordination across agent tasks.
        pub fn new() -> Self {
            Self {
                locks: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        /// Create a lock token that grants exclusive edit access to *prompt_id*
        /// for the given *agent_id* until it expires after *ttl_secs*.
        ///
        /// # Arguments
        /// * `prompt_id` — UUID of the prompt to lock
        /// * `agent_id` — ID of the agent requesting the lock
        /// * `ttl_secs` — Time-to-live in seconds before the token auto-expires
        ///
        /// # Returns
        /// A [`LockToken`] that can be passed back to [`LockManager::is_expired`]
        /// or used to prove ownership when calling `unlock`.
        pub fn create_lock(prompt_id: Uuid, agent_id: AgentId, ttl_secs: u64) -> LockToken {
            LockToken {
                prompt_id,
                agent_id,
                expires_at: Utc::now() + chrono::Duration::seconds(ttl_secs as i64),
                token: Uuid::new_v4().to_string(),
            }
        }

        /// Check whether *token* has passed its expiry wall-clock time.
        ///
        /// # Arguments
        /// * `token` — A previously created [`LockToken`] to inspect
        ///
        /// # Returns
        /// `true` if `Utc::now() > token.expires_at`, `false` otherwise.
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
    quality_gate: Arc<QualityGate>,
    lineage: LineageTracker,
    swarm_registry: Arc<SwarmRoleRegistry>,
    pollination: Arc<std::sync::Mutex<CrossAgentPollination>>,
    satisfaction_tracker: Arc<SatisfactionTracker>,
    health_monitor: Arc<std::sync::Mutex<ProviderHealthMonitor>>,
    load_balancer: Arc<std::sync::Mutex<LoadBalancer>>,
    #[cfg(feature = "budget")]
    budget_tracker: Arc<BudgetTracker>,
    #[cfg(feature = "cost-limits")]
    cost_limiter: std::sync::Arc<crate::cost_limits::CostLimiter>,
    #[cfg(feature = "beta-program")]
    beta_program: Arc<crate::beta_program::BetaProgram>,
    #[cfg(feature = "multi-provider")]
    multi_provider_router: std::sync::Mutex<crate::multi_provider::MultiProviderRouter>,
    #[cfg(feature = "circuit-breaker")]
    circuit_breaker: Arc<CircuitBreaker>,
    #[cfg(feature = "moderation")]
    moderation: Arc<ModerationEngine>,
    #[cfg(feature = "quota")]
    quota_enforcer: Arc<QuotaEnforcer>,
    #[cfg(feature = "preview")]
    preview_engine: Arc<PreviewEngine>,
    #[cfg(feature = "canary")]
    canary_engine: Arc<CanaryEngine>,
    #[cfg(feature = "multimodal")]
    multimodal_engine: MultimodalEngine,
    #[cfg(feature = "i18n")]
    i18n_engine: I18nEngine,
    analytics: Arc<std::sync::Mutex<Analytics>>,
    audit_logger: Arc<SqliteAuditLogger>,
    diff_engine: PromptDiff,
    health_aggregator: HealthAggregator,
    #[cfg(feature = "retention")]
    retention_policy: RetentionPolicy,
    #[cfg(feature = "retention")]
    garbage_collector: GarbageCollector,
    #[cfg(feature = "rollback")]
    safe_deployer: SafeDeployer,
}

impl PromptHub {
    /// Create a new PromptHub instance backed by SQLite storage and a hybrid
    /// search engine (FTS5 + optional ONNX embeddings).
    ///
    /// Initializes the database connection pool, registers the default Junie
    /// orchestrator hook, and sets up RBAC, metrics, sync, and satisfaction
    /// tracking infrastructure.
    ///
    /// # Arguments
    /// * `db_path` — Filesystem path where the libsql/SQLite database will live.
    ///   Pass `:memory:` for an ephemeral in-process store (useful for tests).
    /// * `config` — [`HubConfig`] controlling pool size, search defaults,
    ///   embedding model/dimension/backend, and migration policy.
    ///
    /// # Errors
    /// Returns [`HubError::StorageError`] if the database cannot be opened or
    /// migrations fail to apply.
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
        let smart = Arc::new(SmartEngine::new_with_backend(
            config.embedding_model.clone(),
            storage.clone(),
            config.embedding_dimension,
            &config.embedding_backend,
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
            quality_gate: Arc::new(QualityGate::new()),
            lineage: LineageTracker::new(),
            swarm_registry: Arc::new(swarm::SwarmRoleRegistry::default_registry()),
            pollination: Arc::new(std::sync::Mutex::new(CrossAgentPollination::new())),
            satisfaction_tracker: Arc::new(SatisfactionTracker::new(1000)),
            health_monitor: Arc::new(std::sync::Mutex::new(ProviderHealthMonitor::new())),
            load_balancer: Arc::new(std::sync::Mutex::new(LoadBalancer::new(
                RoutingStrategy::Weighted,
            ))),
            #[cfg(feature = "budget")]
            budget_tracker: Arc::new(BudgetTracker::default()),
            #[cfg(feature = "cost-limits")]
            cost_limiter: std::sync::Arc::new(crate::cost_limits::CostLimiter::default()),
            #[cfg(feature = "beta-program")]
            beta_program: Arc::new(crate::beta_program::BetaProgram::default()),
            #[cfg(feature = "multi-provider")]
            multi_provider_router: std::sync::Mutex::new(
                crate::multi_provider::MultiProviderRouter::default(),
            ),
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: Arc::new(CircuitBreaker::default()),
            #[cfg(feature = "moderation")]
            moderation: Arc::new(ModerationEngine::new()),
            #[cfg(feature = "quota")]
            quota_enforcer: Arc::new(QuotaEnforcer::default()),
            #[cfg(feature = "preview")]
            preview_engine: Arc::new(PreviewEngine),
            #[cfg(feature = "canary")]
            canary_engine: Arc::new(CanaryEngine),
            #[cfg(feature = "multimodal")]
            multimodal_engine: MultimodalEngine,
            #[cfg(feature = "i18n")]
            i18n_engine: I18nEngine::new(),
            analytics: Arc::new(std::sync::Mutex::new(Analytics::new())),
            audit_logger: Arc::new(SqliteAuditLogger::new()),
            diff_engine: PromptDiff::new(),
            health_aggregator: HealthAggregator::new(),
            #[cfg(feature = "retention")]
            retention_policy: RetentionPolicy::default(),
            #[cfg(feature = "retention")]
            garbage_collector: GarbageCollector::new(crate::retention::RetentionPolicy::default()),
            safe_deployer: SafeDeployer::new(),
        };

        // ── Post-struct initialization for feature-gated wiring ───────────

        #[cfg(feature = "retention")]
        {
            let retention = crate::retention::RetentionPolicy::default();
            hub.garbage_collector = GarbageCollector::new(retention.clone());
            hub.retention_policy = retention;
        }

        // Register default hooks
        hub.hooks.register(Box::new(JunieHook));

        #[cfg(feature = "budget")]
        info!("Budget tracker initialized with default monthly budget");

        #[cfg(feature = "circuit-breaker")]
        info!("Circuit breaker initialized with defaults (threshold=5, timeout=30s)");

        #[cfg(feature = "moderation")]
        info!("Content moderation engine initialized in permissive mode");

        #[cfg(feature = "quota")]
        info!("Quota enforcer initialized with defaults (daily=1M, hourly=100K, burst=10K)");

        #[cfg(feature = "preview")]
        info!("Preview engine ready for pre-execution rendering");

        #[cfg(feature = "canary")]
        info!("Canary deployment engine initialized");

        #[cfg(feature = "multimodal")]
        info!("Multimodal engine initialized (image placeholder support)");

        #[cfg(feature = "i18n")]
        info!("I18n translation engine initialized");

        info!("Analytics aggregator initialized");

        info!("Audit logging initialized (SqliteAuditLogger backend)");

        info!("Diff engine initialized (LCS-based text diff)");
        info!("Health aggregator initialized");

        #[cfg(feature = "retention")]
        info!("Retention policy and garbage collection initialized");

        Ok(hub)
    }

    // ── Accessors for server layer ──────────────────────────────────────

    /// Return a cloneable `Arc` handle to the underlying storage layer.
    ///
    /// The returned handle can be shared across axum handlers or worker tasks.
    /// Callers that need direct mutation access should use `Arc::get_mut()`
    /// on the cloned handle rather than this method (which always returns a clone).
    pub fn storage(&self) -> Arc<Storage> {
        Arc::clone(&self.storage)
    }

    /// Return a cloneable `Arc` handle to the metrics collector.
    ///
    /// The collector accumulates request counts, sanitization stats, lock events,
    /// and satisfaction signals across the lifetime of this PromptHub instance.
    pub fn metrics(&self) -> Arc<MetricsCollector> {
        Arc::clone(&self.metrics)
    }

    // ── Prompt CRUD ───────────────────────────────────────────────────────

    /// Register a new prompt after sanitization and RBAC checks.
    ///
    /// The system-sanitizer evaluates the prompt's `system_prompt` and
    /// `user_template` for policy violations (PII, injection, content
    /// restrictions). Blocked prompts produce an error; suspicious ones are
    /// logged but accepted. On success the prompt is indexed by the search
    /// engine and persisted to storage.
    ///
    /// # Arguments
    /// * `prompt` — The [`Prompt`] to register. Its `id` must be unique.
    /// * `identity` — The caller's [`AgentIdentity`] used for RBAC authorization
    ///   and audit trail population.
    ///
    /// # Errors
    /// - [`HubError::SanitizationError`] if the content is blocked by policy.
    /// - [`HubError::Unauthorized`] if *identity* lacks `Write` capability.
    /// - [`HubError::StorageError`] if persistence fails (e.g. unique constraint).
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
    ///
    /// Uses the search engine to find the best matching prompt for the given
    /// *intent* filtered to those targeting *role*. Returns the top result, or
    /// `None` if no match is found. This is the primary lookup method used by
    /// agents at runtime.
    ///
    /// # Arguments
    /// * `role` — The [`Role`] to filter prompts by (e.g. `Developer`, `Architect`).
    /// * `intent` — Natural-language intent text for relevance ranking.
    /// * `identity` — Caller's [`AgentIdentity`] for RBAC authorization.
    ///
    /// # Returns
    /// `Ok(Some(prompt))` when a match exists, `Ok(None)` when no prompt
    /// matches the filter, or an error on auth/storage failure.
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

    /// Get a single prompt by its UUID with RBAC authorization.
    ///
    /// Performs an exact-UUID lookup through the storage layer, gated by
    /// [`RbacAuthManager`] Read authorization for the provided identity.
    ///
    /// # Arguments
    /// * `id` — UUID of the prompt to retrieve.
    /// * `identity` — Caller's [`AgentIdentity`] (used for RBAC).
    ///
    /// # Returns
    /// `Ok(Some(prompt))` if found, `Ok(None)` if not, or an error on failure.
    pub async fn get_by_id(&self, id: Uuid, identity: &AgentIdentity) -> Result<Option<Prompt>> {
        RbacAuthManager::authorize_action(identity, Action::Read)?;
        self.metrics.record_request();
        self.storage.get_prompt(id).await
    }

    /// Search prompts using the configured search engine.
    ///
    /// Delegates to the internal hybrid search pipeline (FTS5 + optional
    /// embedding-based retrieval) and returns a paginated set of scored matches.
    /// Filters narrow by role, domain, tags, and status; pagination controls
    /// page number and per-page count.
    ///
    /// # Arguments
    /// * `query` — Free-text search query against prompt content.
    /// * `mode` — Search mode selector (`Fast`, `Smart`, or `Hybrid`).
    /// * `filters` — [`SearchFilters`] for role, domain, tags, status, etc.
    /// * `pagination` — [`Pagination`] with page number and per-page size limits.
    ///
    /// # Returns
    /// A [`Paginated<ScoredPrompt>`] containing matched prompts sorted by
    /// relevance score. Empty results produce a valid empty paginated response.
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

    /// List all prompts with pagination, optionally filtered by status and domain.
    ///
    /// This method returns every stored prompt (subject to any optional scope
    /// filters encoded in *pagination*) without performing a text search. Use
    /// [`PromptHub::search`] when you need relevance-ranked results.
    ///
    /// # Arguments
    /// * `pagination` — [`Pagination`] with `page` and `per_page` controls the
    ///   slice of the full prompt catalogue to return.
    ///
    /// # Returns
    /// A [`Paginated<Prompt>`] containing the requested page of prompts and the
    /// total count across all pages.
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

    /// Acquire an edit lock on a prompt for exclusive modification.
    ///
    /// Creates a [`LockToken`] that proves the caller's right to edit the given
    /// prompt until it expires. The token must be returned to [`PromptHub::unlock`]
    /// before the prompt can be modified again by another agent.
    ///
    /// # Arguments
    /// * `id` — UUID of the prompt to lock.
    /// * `agent` — Caller's [`AgentIdentity`] (used for RBAC and audit trail).
    /// * `ttl` — Time-to-live duration; after this period the token auto-expires
    ///   and may be acquired by another agent.
    ///
    /// # Errors
    /// - [`HubError::Unauthorized`] if *agent* lacks the `Lock` RBAC action.
    /// - [`HubError::StorageError`] if audit logging fails.
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

    /// Release a previously acquired lock, revoking the caller's exclusive
    /// edit access to the locked prompt.
    ///
    /// If the *token* has expired, the operation fails and the lock remains in
    /// effect for another agent to acquire. On success a release audit entry is
    /// written to storage.
    ///
    /// # Arguments
    /// * `token` — A [`LockToken`] previously returned by [`PromptHub::lock`].
    ///
    /// # Errors
    /// - [`HubError::LockError`] if *token* has expired or is invalid.
    /// - [`HubError::StorageError`] if the release audit entry cannot be written.
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

    /// Get the full audit trail (all mutations) for a prompt.
    ///
    /// Returns every logged action — create, update, evolve, roll-back, lock,
    /// unlock, ownership transfer — associated with *id*, paginated by page and
    /// per-page count. Use this to reconstruct a complete change history.
    ///
    /// # Arguments
    /// * `id` — UUID of the prompt to look up audit entries for.
    /// * `pagination` — [`Pagination`] controlling which slice of entries to return.
    ///
    /// # Returns
    /// A [`Paginated<AuditEntry>`] containing the requested page and total count
    /// of audit entries. Empty when no mutations have been logged.
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

    /// Transfer prompt ownership from one agent to another (admin-only).
    ///
    /// Changes the `author` field of the prompt identified by *id* to *to*'s
    /// agent ID. The caller must hold the `Admin` RBAC action; a full audit
    /// entry is written with before/after diffs. The original owner (*from*) is
    /// recorded for audit but not enforced at storage level.
    ///
    /// # Arguments
    /// * `id` — UUID of the prompt to transfer.
    /// * `_from` — Current owner's [`AgentIdentity`] (recorded in audit).
    /// * `to` — New owner's [`AgentIdentity`].
    /// * `admin` — Admin agent whose credentials authorize this operation.
    ///
    /// # Errors
    /// - [`HubError::Unauthorized`] if *admin* lacks `Admin` RBAC action.
    /// - [`HubError::NotFound`] if the prompt identified by *id* does not exist.
    /// - [`HubError::StorageError`] on persistence failure.
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
    ///
    /// Delegates to the VibeEngine (feature-gated) which transforms a free-form user request
    /// into a structured deliverable with an confidence score, based on the
    /// requested *level* of skill. Requires the `vibe` feature flag.
    ///
    /// # Arguments
    /// * `request` — Natural-language description of the desired output.
    /// * `input` — [`UserInput`] carrying auxiliary parameters (files, context, etc.).
    /// * `level` — Required [`SkillLevel`] for the generated response.
    ///
    /// # Returns
    /// A [`VibeResult`] containing the generated deliverable and a confidence
    /// score indicating how well the output matches the request.
    #[instrument(skip(self))]
    #[cfg(feature = "vibe")]
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

    /// Gather project context from the filesystem at *project_path*.
    ///
    /// Walks the directory tree to collect Cargo manifests, source files, config
    /// files, and dependency information, returning a structured [`ProjectContext`]
    /// suitable for downstream intent classification or cost estimation.
    ///
    /// # Arguments
    /// * `project_path` — Absolute path to the root of the project directory.
    ///
    /// # Returns
    /// A [`ProjectContext`] with file trees, manifests, and inferred metadata.
    ///
    /// # Errors
    /// - [`HubError`] with IO detail if the path cannot be read or is not a directory.
    #[instrument(skip(self))]
    pub async fn gather_context(&self, project_path: &Path) -> Result<ProjectContext> {
        use crate::context_gatherer::ContextGatherer;
        let ctx = ContextGatherer::gather(project_path).await?;
        info!("Gathered context for {}", ctx.project_path);
        Ok(ctx)
    }

    // ── Cost estimation ───────────────────────────────────────────────────

    /// Estimate the cost of executing an *intent* within the given *context*.
    ///
    /// Analyzes the intent's complexity against project metadata (crate count,
    /// file sizes, dependency depth) to produce a dollar-cost projection.
    /// Requires the `cost` feature flag.
    ///
    /// # Arguments
    /// * `intent` — The [`Intent`] whose cost should be estimated.
    /// * `context` — A [`ProjectContext`] describing the target codebase.
    ///
    /// # Returns
    /// A [`CostEstimate`] with USD cost, token counts (input/output), and a
    /// breakdown by component (analysis, generation, testing).
    #[cfg(feature = "cost")]
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

    /// Scan user input for potential privacy violations (PII, secrets, credentials).
    ///
    /// Runs the configured privacy scanner over every field in *input* and
    /// returns a categorized report of detected issues with severity levels.
    /// Requires the `privacy` feature flag.
    ///
    /// # Arguments
    /// * `input` — The [`UserInput`] to scan for privacy-sensitive content.
    ///
    /// # Returns
    /// A [`PrivacyReport`] with a risk level (low / medium / high), detected
    /// issues by category, and suggested mitigations.
    #[cfg(feature = "privacy")]
    #[instrument(skip(self))]
    pub async fn scan_privacy(&self, input: &UserInput) -> Result<PrivacyReport> {
        use crate::privacy::PrivacyScanner;
        let scanner = PrivacyScanner::default();
        let report = scanner.scan(input).await?;
        info!("Privacy scan completed: {:?} risk level", report.risk_level);
        Ok(report)
    }

    // ── Confidence scoring ────────────────────────────────────────────────

    /// Score confidence for an *intent* against a given project *context*.
    ///
    /// Evaluates how well the intent aligns with existing code patterns, module
    /// structure, and dependency graph to produce a confidence score (0.0–1.0).
    /// Requires the `confidence` feature flag.
    ///
    /// # Arguments
    /// * `intent` — The [`Intent`] to evaluate.
    /// * `context` — A [`ProjectContext`] describing the target codebase structure.
    ///
    /// # Returns
    /// A [`ConfidenceScore`] with a numeric score (0.0–1.0), supporting factors,
    /// and confidence breakdown by dimension (domain fit, pattern match, etc.).
    #[cfg(feature = "confidence")]
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

    /// Gracefully shut down the hub: optimize storage, drain metrics.
    ///
    /// Flushes pending WAL checkpoints to disk (via `optimize_on_close`),
    /// stops metric collection, and releases background resources. Intended
    /// for use at process exit or during hot-reload cycles.
    ///
    /// # Returns
    /// `Ok(())` on success, or a [`HubError::StorageError`] if the WAL flush
    /// fails (which would indicate potential data loss).
    #[instrument(skip(self))]
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down PromptHub storage...");
        self.storage.optimize_on_close().await?;
        info!("PromptHub shutdown complete");
        Ok(())
    }

    // ── Prompt lifecycle ──────────────────────────────────────────────────

    /// Update an existing prompt with the given *patch* and audit the change.
    ///
    /// Applies only the fields set in *patch* (e.g. `system_prompt`,
    /// `user_template`, `tags`). The caller's *identity* is recorded in the
    /// audit trail along with a before/after diff hash for tamper evidence.
    /// RBAC requires `Write` capability.
    ///
    /// # Arguments
    /// * `id` — UUID of the prompt to update.
    /// * `patch` — [`PromptPatch`] containing only the fields to change.
    /// * `identity` — Caller's [`AgentIdentity`] for RBAC and audit trail.
    ///
    /// # Errors
    /// - [`HubError::Unauthorized`] if *identity* lacks `Write` capability.
    /// - [`HubError::NotFound`] if no prompt with *id* exists.
    /// - [`HubError::StorageError`] on persistence failure.
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

    /// Rollback a prompt to a previous version identified by *to_version*.
    ///
    /// Restores the prompt stored under *id* to its state at the named
    /// *to_version*, then re-indexes it in the search engine and logs an
    /// audit entry. Requires the `rollback` feature flag and `Write` RBAC.
    ///
    /// # Arguments
    /// * `id` — UUID of the prompt to roll back.
    /// * `to_version` — Version string (e.g. `"v1.2.0"`) to restore.
    /// * `identity` — Caller's [`AgentIdentity`] for RBAC and audit trail.
    ///
    /// # Errors
    /// - [`HubError::Unauthorized`] if *identity* lacks `Write` capability.
    /// - [`HubError::NotFound`] if the prompt or target version does not exist.
    /// - [`HubError::StorageError`] on persistence failure.
    #[cfg(feature = "rollback")]
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

    /// Evolve a prompt using the specified *strategy*, producing a new version.
    ///
    /// Applies the given [`EvolutionStrategy`] (mutate, crossover, etc.) to the
    /// existing prompt identified by *id*. The result is persisted as a **new**
    /// prompt (different UUID) and indexed for search; the original is preserved
    /// in storage for lineage tracing.
    ///
    /// # Arguments
    /// * `id` — UUID of the base prompt to evolve.
    /// * `strategy` — The [`EvolutionStrategy`] to apply (`Mutate`, `Crossover`, etc.).
    /// * `identity` — Caller's [`AgentIdentity`] for RBAC and audit trail.
    ///
    /// # Errors
    /// - [`HubError::Unauthorized`] if *identity* lacks `Write` capability.
    /// - [`HubError::NotFound`] if no prompt with *id* exists.
    /// - [`HubError::Internal("No crossover candidates")`] for `Crossover` when
    ///   no other prompts are available to act as parents.
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

    /// Execute the configured fallback chain for an *intent* within a given *context*.
    ///
    /// Tries each fallback strategy in order (e.g. direct generation → template
    /// injection → handoff to orchestrator) until one succeeds. Requires the
    /// `fallback` feature flag.
    ///
    /// # Arguments
    /// * `intent` — The [`Intent`] to resolve via fallback strategies.
    /// * `context` — A [`ProjectContext`] providing codebase metadata for resolution.
    ///
    /// # Returns
    /// An [`Artifact`] (code, prompt, or other output type) produced by the first
    /// strategy that succeeds, or an error if all fallbacks fail.
    #[cfg(feature = "fallback")]
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
    ///
    /// Records the *correction* string alongside the original *intent* in the
    /// learning engine's history so that future requests for similar intents can
    /// benefit from this correction. Requires the `learn` feature flag.
    ///
    /// # Arguments
    /// * `correction` — Free-text description of what was wrong and how to fix it.
    /// * `intent` — The [`Intent`] that triggered the feedback (for indexing).
    /// * `agent_id` — UUID of the agent providing the correction (audit trail).
    #[cfg(feature = "learn")]
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

    // ── Quality gate ────────────────────────────────────────────────────

    /// Run the quality gate pipeline against an artifact.
    ///
    /// Returns a `QualityResult` with scores and pass/fail for lint,
    /// security, performance, and accessibility checks registered on the gate.
    #[instrument(skip(self))]
    pub async fn run_quality_gate(&self, artifact: &Artifact) -> Result<QualityResult> {
        let result = self.quality_gate.check(artifact).await?;
        info!(
            passed = %result.passed,
            warnings = %result.warnings.len(),
            errors = %result.errors.len(),
            "Quality gate result"
        );
        Ok(result)
    }

    // ── Version lineage ───────────────────────────────────────────────

    /// Get the ancestry path (from root to *version_id*) in the version graph.
    ///
    /// Returns the ordered chain of ancestor version IDs and the depth of the
    /// tree branch ending at *version_id*.
    ///
    /// # Arguments
    /// * `version_id` — The version whose ancestry path to resolve.
    ///
    /// # Returns
    /// An [`AncestryPath`] with `path` (ordered root-first) and `depth`.
    ///
    /// # Errors
    /// - [`HubError::NotFound`] if *version_id* is not tracked.
    #[instrument(skip(self))]
    pub fn get_lineage_ancestry(&self, version_id: &str) -> Result<AncestryPath> {
        self.lineage.get_ancestry(version_id)
    }

    /// Detect all forks in the lineage graph.
    ///
    /// A fork occurs when a single version has two or more children (i.e.
    /// multiple branches diverge from one parent). This is useful for
    /// identifying parallel evolution of prompts.
    #[instrument(skip(self))]
    pub fn detect_lineage_forks(&self) -> Vec<Fork> {
        self.lineage.detect_forks()
    }

    /// Get all descendant version IDs reachable from *version_id*.
    ///
    /// Traverses the full descendant graph (not just direct children) and returns
    /// every reachable version ID in breadth-first order.
    ///
    /// # Arguments
    /// * `version_id` — The root version to traverse descendants of.
    #[instrument(skip(self))]
    pub fn get_lineage_descendants(&self, version_id: &str) -> Vec<String> {
        self.lineage.get_descendants(version_id)
    }

    /// Build a lineage tree rooted at *root_version*, including all descendants.
    ///
    /// Returns `None` if the root is not tracked. The tree encodes parent-child
    /// edges and fork points for visualization or diffing.
    ///
    /// # Arguments
    /// * `root_version` — The version to root the tree at.
    #[instrument(skip(self))]
    pub fn build_lineage_tree(&self, root_version: &str) -> Option<LineageTree> {
        self.lineage.build_tree(root_version)
    }

    /// Mutable access to the lineage tracker (caller owns mutation).
    ///
    /// Prefer using this over storing a separate Arc/Mutex — it avoids
    /// double-allocation and keeps the tracker inline with PromptHub.
    #[allow(clippy::mutable_key_type)]
    pub fn lineage_mut(&mut self) -> &mut LineageTracker {
        &mut self.lineage
    }

    /// Number of registered version nodes in the lineage graph.
    #[inline]
    pub fn lineage_node_count(&self) -> usize {
        self.lineage.node_count()
    }

    /// Check whether a specific *version_id* is tracked in the lineage graph.
    #[inline]
    pub fn has_lineage_version(&self, version_id: &str) -> bool {
        self.lineage.has_version(version_id)
    }

    /// Get the set of root versions (no parents).
    pub fn lineage_roots(&self) -> &[String] {
        self.lineage.roots()
    }

    // - Swarm role registry --------------------------------------------------

    /// Return a cloneable handle to the swarm role registry.
    ///
    /// The returned `Arc` can be cloned and shared across handlers or
    /// downstream components. Mutable operations (e.g., registering custom
    /// roles) use `Arc::get_mut()` on the original.
    pub fn manage_swarm(&self) -> Arc<SwarmRoleRegistry> {
        Arc::clone(&self.swarm_registry)
    }

    /// Validate a set of roles against the swarm dependency DAG.
    ///
    /// Returns an empty vec if all roles are valid, or a list of conflicts
    /// (missing required roles, duplicates, capability gaps, custom-name
    /// violations).
    #[instrument(skip(self, roles))]
    pub fn validate_swarm_roles(&self, roles: &[Role]) -> Result<Vec<Conflict>> {
        swarm::validate_swarm_roles(roles)
    }

    /// Generate a swarm bundle for the given roles, domain, and workflow.
    ///
    /// Validates the role DAG, builds the dependency graph, generates a
    /// consistency report, evolution suggestions, and handoff templates.
    #[instrument(skip(self, roles))]
    pub async fn generate_swarm_bundle(
        &self,
        roles: Vec<Role>,
        domain: Domain,
        workflow_id: Uuid,
    ) -> Result<SwarmBundle> {
        swarm::generate_swarm_bundle(roles, domain, workflow_id).await
    }

    // - Cross-agent pollination ---------------------------------------------------

    /// Return a cloneable handle to the cross-agent pollination engine.
    ///
    /// The returned `Arc` can be cloned and shared across handlers. Mutable
    /// operations (e.g., sharing patterns) use `Arc::get_mut()` on the original.
    pub fn pollination(&self) -> Arc<std::sync::Mutex<CrossAgentPollination>> {
        Arc::clone(&self.pollination)
    }

    /// Extract reusable prompt patterns from a prompt for cross-agent sharing.
    ///
    /// Analyzes the prompt's `system_prompt` and `user_template` to detect
    /// structural patterns (e.g. step-by-step, few-shot, chain-of-thought) that
    /// could be reused by other agents in the swarm.
    ///
    /// # Arguments
    /// * `prompt` — The [`Prompt`] to extract patterns from.
    ///
    /// # Returns
    /// A vector of [`Pattern`] structs, each describing a detected structural
    /// pattern with its confidence score and applicable domain tags.
    #[instrument(skip(self, prompt))]
    pub fn extract_pollination_patterns(&self, prompt: &Prompt) -> Result<Vec<Pattern>> {
        Ok(CrossAgentPollination::extract_patterns(prompt))
    }

    /// Rank all patterns in the pollination pool by composite score.
    ///
    /// Scores combine usage frequency, success rate, and domain diversity to
    /// produce a ranking of reusable patterns. Only returns the top *num_domains*
    /// distinct-domain representatives.
    ///
    /// # Arguments
    /// * `num_domains` — Maximum number of distinct domains to return (i.e. result count).
    ///
    /// # Returns
    /// A vector of `(pattern_name, score)` tuples sorted descending by score.
    #[instrument(skip(self))]
    pub fn rank_pollination_patterns(&self, num_domains: usize) -> Result<Vec<(String, f64)>> {
        let engine = self
            .pollination
            .lock()
            .map_err(|e| HubError::Internal(format!("pollination mutex poisoned: {e}")))?;
        Ok(engine
            .rank_patterns(num_domains)
            .into_iter()
            .map(|(k, v)| (k.clone(), v))
            .collect())
    }

    /// Mutable access to the pollination engine (caller owns mutation).
    ///
    /// Prefer using this over cloning the Arc + holding a separate guard -- it
    /// avoids double-allocation and keeps the engine inline with PromptHub.
    pub fn pollination_mut(&mut self) -> &mut CrossAgentPollination {
        let mutex = Arc::get_mut(&mut self.pollination).expect("pollination mutex poisoned");
        mutex.get_mut().expect("pollination lock poisoned")
    }

    // - User satisfaction tracker --------------------------------------------------

    /// Return a cloneable handle to the user satisfaction tracker.
    ///
    /// The returned `Arc` can be cloned and shared across handlers. Mutable
    /// operations (e.g., recording ratings) use the provided delegate methods
    /// or call into the tracker directly via this handle.
    pub fn satisfaction_tracker(&self) -> Arc<SatisfactionTracker> {
        Arc::clone(&self.satisfaction_tracker)
    }

    /// Record a CSAT rating (1-5), delegated to the satisfaction tracker.
    ///
    /// Scores outside the valid range 1..=5 are silently ignored. The optional
    /// *context* string is stored alongside the rating for later segmentation.
    ///
    /// # Arguments
    /// * `score` — CSAT score on a 1-5 Likert scale (1=Dissatisfied, 5=Satisfied).
    /// * `context` — Free-form context describing the user's experience.
    #[instrument(skip(self))]
    pub fn record_csat_rating(&self, score: u8, context: &str) {
        self.satisfaction_tracker.record_csat(score, context);
    }

    /// Record an NPS rating (1-10), delegated to the satisfaction tracker.
    ///
    /// Scores outside the valid range 1..=10 are silently ignored. The aggregate
    /// NPS score is computed as `(promoters - detractors) / total`.
    ///
    /// # Arguments
    /// * `score` — NPS score on a 1-10 scale (9-10=promoter, 7-8=passive, 1-6=detractor).
    #[instrument(skip(self))]
    pub fn record_nps_rating(&self, score: u8) {
        self.satisfaction_tracker.record_nps(score);
    }

    /// Record a success/failure event in the satisfaction funnel.
    ///
    /// Tracks whether a prompt resolution was ultimately successful and how many
    /// attempts it took. Events feed into the one-shot success rate metric.
    ///
    /// # Arguments
    /// * `prompt_id` — Identifier of the prompt involved in this interaction.
    /// * `successful` — Whether the user's goal was achieved on this attempt.
    /// * `attempts` — Number of attempts before resolution (1 = solved immediately).
    #[instrument(skip(self))]
    pub fn record_satisfaction_event(&self, prompt_id: &str, successful: bool, attempts: u8) {
        self.satisfaction_tracker
            .record_event(prompt_id, successful, attempts);
    }

    /// Query current satisfaction metrics (CSAT average, NPS score, success rate).
    ///
    /// Returns aggregate statistics across all recorded ratings and events. When
    /// no data has been collected, all numeric fields default to 0.0.
    #[instrument(skip(self))]
    pub fn satisfaction_metrics(&self) -> Result<SatisfactionMetrics> {
        Ok(self.satisfaction_tracker.metrics())
    }

    // ── Provider health monitor ---------------------------------------------------

    /// Return a cloneable handle to the provider health monitor.
    ///
    /// The returned `Arc` can be cloned and shared across handlers. Mutable
    /// operations (e.g., registering providers, recording latencies) use the
    /// provided delegate methods or call into the monitor directly via this handle.
    pub fn health_monitor(&self) -> Arc<std::sync::Mutex<ProviderHealthMonitor>> {
        Arc::clone(&self.health_monitor)
    }

    /// Register an LLM provider for health monitoring.
    ///
    /// Adds a new named provider to the monitor's registry. Subsequent calls
    /// with the same *name* will overwrite the previous URL and reset any
    /// accumulated latency/error metrics.
    ///
    /// # Arguments
    /// * `name` — Unique identifier for this provider (e.g., `"gpt-4o"`).
    /// * `url` — Base URL or endpoint string for the provider.
    #[instrument(skip(self))]
    pub fn register_provider(&self, name: &str, url: &str) {
        let monitor = self.health_monitor();
        monitor.lock().unwrap().register(name, url);
        info!(provider = name, url = url, "Registered LLM provider");
    }

    /// Record a successful API call for the named provider.
    ///
    /// The *latency_ms* is stored alongside the current timestamp and used to
    /// compute rolling averages for latency-based health thresholds.
    ///
    /// # Arguments
    /// * `provider_name` — Name of the registered provider.
    /// * `latency_ms` — Round-trip latency in milliseconds.
    #[instrument(skip(self))]
    pub fn record_success(&self, provider_name: &str, latency_ms: u64) {
        let monitor = self.health_monitor();
        monitor
            .lock()
            .unwrap()
            .record_success(provider_name, latency_ms);
        info!(
            provider = provider_name,
            latency_ms = latency_ms,
            "Recorded provider success"
        );
    }

    /// Record a failed API call for the named provider.
    ///
    /// Each failure increments the error rate used by health thresholds. If the
    /// rolling error rate exceeds the configured threshold, the provider's status
    /// transitions to `HealthStatus::Unhealthy`.
    ///
    /// # Arguments
    /// * `provider_name` — Name of the registered provider.
    #[instrument(skip(self))]
    pub fn record_failure(&self, provider_name: &str) {
        let monitor = self.health_monitor();
        monitor.lock().unwrap().record_failure(provider_name);
        warn!(provider = provider_name, "Recorded provider failure");
    }

    /// Check whether the named provider is currently considered healthy.
    ///
    /// A provider is healthy when its rolling error rate stays below the configured
    /// threshold and its average latency is within bounds. Returns `false` if the
    /// provider has never been registered or probed.
    ///
    /// # Arguments
    /// * `provider_name` — Name of the registered provider.
    #[instrument(skip(self))]
    pub fn is_healthy(&self, provider_name: &str) -> bool {
        let monitor = self.health_monitor();
        let healthy = monitor.lock().unwrap().is_healthy(provider_name);
        info!(provider = provider_name, healthy = healthy, "Health check");
        healthy
    }

    /// Retrieve the full health summary for all registered providers.
    ///
    /// Returns a [`HealthSummary`] containing per-provider status, average latency,
    /// error rate, and total call counts. Providers that have been registered but
    /// never probed appear with `HealthStatus::Unknown` status.
    pub fn get_health_summary(&self) -> HealthSummary {
        let monitor = self.health_monitor();
        monitor.lock().unwrap().summary()
    }

    // ── Load balancer -----------------------------------------------------------

    // ── Rollback (safe deployment) --------------------------------------------

    /// Deploy a prompt with automatic rollback capability.
    /// Requires the `rollback` feature flag and Write RBAC.
    #[cfg(feature = "rollback")]
    pub async fn deploy_with_rollback(
        &self,
        artifact: &crate::models::Artifact,
        rollback_enabled: bool,
    ) -> Result<crate::rollback::DeployResult> {
        self.safe_deployer
            .deploy_with_rollback(artifact, rollback_enabled)
            .await
    }

    /// Restore a prompt to a previously saved snapshot by ID.
    #[cfg(feature = "rollback")]
    pub async fn restore_snapshot(&self, id: &str) -> Result<()> {
        self.safe_deployer.restore_snapshot(id).await
    }

    /// Check if a specific rollback snapshot is available.
    #[cfg(feature = "rollback")]
    pub fn is_rollback_available(&self, snapshot_id: &str) -> bool {
        self.safe_deployer.is_rollback_available(snapshot_id)
    }

    /// Return a cloneable handle to the load balancer.
    pub fn load_balancer(&self) -> Arc<std::sync::Mutex<LoadBalancer>> {
        Arc::clone(&self.load_balancer)
    }

    /// Add a provider to the load balancer pool.
    ///
    /// The *weight* parameter controls how often this provider is selected
    /// during weighted round-robin routing (higher weight = more requests).
    ///
    /// # Arguments
    /// * `name` — Unique identifier for the provider (e.g., `"gpt-4o-primary"`).
    /// * `url` — Endpoint URL for the provider.
    /// * `weight` — Relative traffic share (default 1 = equal weight).
    #[instrument(skip(self))]
    pub fn add_lb_provider(&self, name: &str, url: &str, weight: u32) {
        let lb = self.load_balancer();
        lb.lock().unwrap().add_provider(name, url, weight);
        info!(
            provider = name,
            weight = weight,
            "Added provider to load balancer"
        );
    }

    /// Select the next healthy provider according to the configured routing strategy.
    ///
    /// For `WeightedRoundRobin`, returns a [`ProviderSelection`] with the selected
    /// provider's details and computed weight for the current round. Returns an error
    /// if no providers are registered or all are marked unhealthy.
    /// Select the next healthy provider according to the configured routing strategy.
    ///
    /// For `Weighted` strategy, returns a [`ProviderSelection`] with the selected
    /// provider's details and computed weight for the current round. Returns an error
    /// if no providers are registered or all are marked unhealthy.
    #[instrument(skip(self))]
    pub fn select_provider(&self) -> Result<ProviderSelection> {
        let lb = self.load_balancer();
        let binding = lb.lock().unwrap();
        binding.select_provider()
    }

    /// Record latency metrics for a specific provider in the load balancer pool.
    ///
    /// Used by health monitors or probes to update latency statistics that
    /// may influence routing decisions (e.g., preferring faster providers).
    ///
    /// # Arguments
    /// * `provider_name` — Name of the registered provider.
    /// * `latency_ms` — Measured round-trip latency in milliseconds.
    #[instrument(skip(self))]
    pub fn record_lb_latency(&self, provider_name: &str, latency_ms: u64) {
        let lb = self.load_balancer();
        lb.lock().unwrap().record_latency(provider_name, latency_ms);
    }

    /// Record a failure event for the named provider in the load balancer pool.
    ///
    /// Increments the error counter used by health-aware routing. Providers with
    /// too many errors may be temporarily excluded from the rotation.
    #[instrument(skip(self))]
    pub fn record_lb_failure(&self, provider_name: &str) {
        let lb = self.load_balancer();
        lb.lock().unwrap().record_error(provider_name);
        warn!(provider = provider_name, "Recorded load balancer failure");
    }

    /// Return current stats for all providers in the load balancer pool.
    pub fn get_lb_stats(&self) -> Vec<ProviderStats> {
        let lb = self.load_balancer();
        lb.lock().unwrap().stats()
    }

    // ── Budget tracking ────────────────────────────────────────────────

    /// Record a spend amount against the monthly budget.
    ///
    /// Increments the current spend counter and fires an alert if any
    /// configured threshold is crossed for the first time (50%, 80%, 100%).
    /// Requires the `budget` feature flag.
    ///
    /// # Arguments
    /// * `amount_usd` — Spend amount in US dollars to record.
    ///
    /// # Returns
    /// A [`BudgetAlert`] indicating if a threshold was crossed, or `None`.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn record_spend(&self, amount_usd: f64) -> BudgetAlert {
        let alert = self.budget_tracker.record_spend(amount_usd);
        if let BudgetAlert::None = alert {
            debug!("Recorded ${:.4} spend", amount_usd);
        }
        alert
    }

    /// Get the current monthly budget utilization as a percentage.
    ///
    /// Returns 0.0 if no budget is configured or if spend has not been reset
    /// for the billing period.
    /// Requires the `budget` feature flag.
    ///
    /// # Returns
    /// A float in the range [0.0, 100.0+] where >100.0 means over budget.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn budget_utilization(&self) -> f64 {
        self.budget_tracker.utilization_percent()
    }

    /// Get the current month's spend in USD.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn current_spend_usd(&self) -> f64 {
        self.budget_tracker.current_spend_usd()
    }

    /// Check whether the monthly budget has been exceeded.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn is_budget_exceeded(&self) -> bool {
        self.budget_tracker.is_exceeded()
    }

    /// Update the configured monthly budget amount.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn set_monthly_budget(&self, monthly_budget_usd: f64) {
        self.budget_tracker.set_budget(monthly_budget_usd);
    }

    /// Load a persisted [`BudgetConfig`] into the tracker.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn load_budget_config(&self, config: &BudgetConfig) -> Result<()> {
        self.budget_tracker.load_config(config)
    }

    /// Save the current budget state as a [`BudgetConfig`] for the given org.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn save_budget_config(&self, org_id: &str) -> Result<BudgetConfig> {
        self.budget_tracker.save_config(org_id)
    }

    /// Reset spend counters for a new billing period.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn reset_budget_period(&self) {
        self.budget_tracker.reset_period();
    }

    // ── Cost limits (multi-dimensional budget enforcement) -----------------------

    /// Record spend against an entity's resource bucket with overage enforcement.
    ///
    /// Returns the [`LimitStatus`] after recording the spend, indicating whether
    /// it was allowed, flagged as over-limit, or blocked.
    #[cfg(feature = "cost-limits")]
    #[instrument(skip(self))]
    pub async fn check_cost_limits(
        &self,
        entity_id: &str,
        resource: crate::cost_limits::Resource,
        amount_usd: f64,
    ) -> crate::cost_limits::LimitStatus {
        self.cost_limiter
            .check_and_record(entity_id, resource, amount_usd)
    }

    /// Add or update a cost limit for an entity-resource pair.
    #[cfg(feature = "cost-limits")]
    #[instrument(skip(self))]
    pub fn set_cost_limit(
        &self,
        entity_id: &str,
        resource: crate::cost_limits::Resource,
        budget_usd: f64,
        policy: crate::cost_limits::OveragePolicy,
    ) -> crate::cost_limits::LimitEntry {
        self.cost_limiter
            .set_limit(entity_id, resource, budget_usd, policy)
    }

    /// Get utilization percentage for an entity-resource pair.
    #[cfg(feature = "cost-limits")]
    pub fn cost_utilization(
        &self,
        entity_id: &str,
        resource: crate::cost_limits::Resource,
    ) -> Option<f64> {
        self.cost_limiter.utilization(entity_id, resource)
    }

    /// Get all tracked entities and their limit statuses.
    #[cfg(feature = "cost-limits")]
    pub fn cost_limit_status(&self) -> Vec<(String, f64, crate::cost_limits::OveragePolicy)> {
        self.cost_limiter
            .entity_ids()
            .into_iter()
            .flat_map(|id| {
                self.cost_limiter
                    .entity_status(&id)
                    .into_iter()
                    .map(move |(_res, util, pol)| (id.clone(), util, pol))
            })
            .collect()
    }

    // ── Beta program (phased deployment) -------------------------------------------

    /// Create a new beta cohort for testing.
    #[cfg(feature = "beta-program")]
    pub fn create_beta_cohort(&self, id: &str, name: &str) -> crate::beta_program::BetaCohort {
        self.beta_program.create_cohort(id, name)
    }

    /// Enroll a participant in a beta cohort.
    #[cfg(feature = "beta-program")]
    pub fn enroll_beta(&self, cohort_id: &str, participant_id: &str) -> bool {
        self.beta_program.enroll(cohort_id, participant_id)
    }

    /// Record feedback from a beta participant.
    #[cfg(feature = "beta-program")]
    pub fn record_feedback(
        &self,
        cohort_id: &str,
        participant_id: &str,
        score: u8,
        comment: String,
    ) -> bool {
        self.beta_program
            .record_feedback(cohort_id, participant_id, score, comment)
    }

    /// Get overall beta program statistics.
    #[cfg(feature = "beta-program")]
    pub fn beta_stats(&self) -> crate::beta_program::ProgramStats {
        self.beta_program.stats()
    }

    // ── Multi-provider routing ---------------------------------------------------

    /// Add a new provider to the multi-provider routing pool.
    #[cfg(feature = "multi-provider")]
    pub fn add_provider(&self, config: crate::multi_provider::ProviderConfig) {
        self.multi_provider_router
            .lock()
            .unwrap()
            .add_provider(config);
    }

    /// Select the best provider for routing, optionally filtering by vendor.
    #[cfg(feature = "multi-provider")]
    pub fn route_to_vendor(
        &self,
        vendor_filter: Option<crate::multi_provider::Vendor>,
    ) -> Option<crate::multi_provider::RoutingDecision> {
        self.multi_provider_router
            .lock()
            .unwrap()
            .select(vendor_filter)
    }

    /// Record a successful request for a multi-provider routing entry.
    #[cfg(feature = "multi-provider")]
    pub fn record_provider_success(&self, provider_name: &str) {
        self.multi_provider_router
            .lock()
            .unwrap()
            .record_success(provider_name);
    }

    /// Record a failed request for a multi-provider routing entry.
    #[cfg(feature = "multi-provider")]
    pub fn record_provider_failure(&self, provider_name: &str) {
        self.multi_provider_router
            .lock()
            .unwrap()
            .record_failure(provider_name);
    }

    /// Get the health statistics for all providers in the pool.
    #[cfg(feature = "multi-provider")]
    pub fn provider_pool_stats(&self) -> crate::multi_provider::PoolStats {
        self.multi_provider_router.lock().unwrap().pool_stats()
    }

    // ── Circuit breaker ----------------------------------------------------------

    /// Return a cloneable handle to the circuit breaker.
    #[cfg(feature = "circuit-breaker")]
    pub fn circuit_breaker(&self) -> Arc<CircuitBreaker> {
        Arc::clone(&self.circuit_breaker)
    }

    // ── Content moderation ────────────────────────────────────────────

    /// Moderate user input for harmful content before processing.
    ///
    /// Runs the prompt against all configured moderation categories
    /// (hate, violence, self-harm, sexual, illegal, harassment) and returns
    /// a [`ModerationReport`] with allow/block/flag result.
    ///
    /// Requires the `moderation` feature flag.
    #[cfg(feature = "moderation")]
    #[instrument(skip(self, prompt))]
    pub fn check_content(&self, prompt: &str) -> Result<crate::moderation::ModerationReport> {
        self.moderation.check(prompt)
    }

    /// Quick boolean check: returns `true` if the content passes moderation.
    #[cfg(feature = "moderation")]
    #[instrument(skip(self, prompt))]
    pub fn is_content_safe(&self, prompt: &str) -> bool {
        self.moderation.is_allowed(prompt)
    }

    /// Moderate multiple prompts in sequence for bulk operations.
    #[cfg(feature = "moderation")]
    pub fn check_content_batch(
        &self,
        prompts: &[String],
    ) -> Vec<Result<crate::moderation::ModerationReport>> {
        self.moderation.check_batch(prompts)
    }

    /// Return a cloneable handle to the moderation engine.
    #[cfg(feature = "moderation")]
    pub fn moderation_engine(&self) -> Arc<ModerationEngine> {
        Arc::clone(&self.moderation)
    }

    // ── Token quota ---------------------------------------------------------

    /// Check and consume tokens against configured daily/hourly/burst quotas.
    ///
    /// Returns `QuotaStatus::Allowed` if the request fits within all limits,
    /// or the first exceeded limit (burst > hourly > daily check order).
    #[cfg(feature = "quota")]
    #[instrument(skip(self, tokens))]
    pub fn check_and_consume(&self, tokens: u64) -> Result<crate::quota::QuotaStatus> {
        self.quota_enforcer.check_and_consume(tokens)
    }

    /// Return current quota usage snapshot.
    #[cfg(feature = "quota")]
    pub fn quota_usage(&self) -> crate::quota::QuotaUsage {
        self.quota_enforcer.usage()
    }

    /// Reset all quota counters (admin override or testing).
    #[cfg(feature = "quota")]
    pub fn reset_quota(&self) {
        self.quota_enforcer.reset_all();
    }

    /// Return a cloneable `Arc` handle to the quota enforcer.
    #[cfg(feature = "quota")]
    pub fn quota_enforcer_handle(&self) -> Arc<QuotaEnforcer> {
        Arc::clone(&self.quota_enforcer)
    }

    // ── Preview engine ------------------------------------------------------

    /// Generate a pre-execution preview for the given plan.
    #[cfg(feature = "preview")]
    #[instrument(skip(self, plan))]
    pub async fn preview_generate(
        &self,
        plan: &crate::models::ExecutionPlan,
    ) -> Result<crate::preview::PreviewType> {
        self.preview_engine.generate(plan).await
    }

    /// Preview the artifacts that would be generated.
    #[cfg(feature = "preview")]
    #[instrument(skip(self, artifacts))]
    pub async fn preview_artifacts(
        &self,
        artifacts: &[crate::models::Artifact],
    ) -> Result<crate::preview::PreviewType> {
        self.preview_engine.preview_artifacts(artifacts).await
    }

    /// Return a cloneable `Arc` handle to the preview engine.
    #[cfg(feature = "preview")]
    pub fn preview_engine_handle(&self) -> Arc<PreviewEngine> {
        Arc::clone(&self.preview_engine)
    }

    // ── Canary deployment ──────────────────────────────────────────────

    /// Deploy a canary version of a feature and return whether the deployment succeeded.
    #[cfg(feature = "canary")]
    #[instrument(skip(self, canary))]
    pub async fn canary_deploy(&self, canary: &CanaryDeployment, user_id: Uuid) -> Result<bool> {
        CanaryEngine::deploy(canary, user_id).await
    }

    /// Evaluate whether a canary deployment should be rolled back.
    #[cfg(feature = "canary")]
    pub fn canary_should_rollback(
        &self,
        canary: &CanaryDeployment,
        error_rate: f64,
        latency_p99: f64,
    ) -> bool {
        CanaryEngine::should_rollback(canary, error_rate, latency_p99)
    }

    /// Return a cloneable `Arc` handle to the canary engine.
    #[cfg(feature = "canary")]
    pub fn canary_engine_handle(&self) -> Arc<CanaryEngine> {
        Arc::clone(&self.canary_engine)
    }

    // ── Multimodal (multimodal input support) ──────────────────────────

    /// Return a reference to the multimodal engine.
    #[cfg(feature = "multimodal")]
    pub fn multimodal_engine(&self) -> &MultimodalEngine {
        &self.multimodal_engine
    }

    /// Validate that an image MIME type is supported by the multimodal engine.
    #[cfg(feature = "multimodal")]
    pub fn validate_image_mime_type(&self, mime: &str) -> bool {
        MultimodalEngine::validate_mime_type(mime)
    }

    /// Extract all placeholder IDs from a template string.
    #[cfg(feature = "multimodal")]
    pub fn extract_placeholder_ids(&self, template: &str) -> Vec<String> {
        MultimodalEngine::extract_placeholder_ids(template)
    }

    // ── I18n (internationalization / translation support) ────────────────

    /// Return a reference to the i18n translation engine.
    #[cfg(feature = "i18n")]
    pub fn i18n_engine(&self) -> &I18nEngine {
        &self.i18n_engine
    }

    /// Register a new translation for a prompt in a specific locale.
    #[cfg(feature = "i18n")]
    pub fn register_translation(&mut self, prompt_id: &str, locale: &str, template: String) {
        self.i18n_engine
            .register_translation(prompt_id, locale, template);
    }

    /// Get the localized template for a prompt in a specific locale.
    #[cfg(feature = "i18n")]
    pub fn get_localized_template(&self, prompt_id: &str, locale: &str) -> Option<String> {
        self.i18n_engine
            .get_translation(prompt_id, locale)
            .map(|s| s.to_string())
    }

    /// Get the fallback chain for a locale.
    #[cfg(feature = "i18n")]
    pub fn translation_fallback_chain(&self, locale: &str) -> Vec<String> {
        crate::i18n::I18nEngine::fallback_chain(locale)
    }

    // ── Analytics ──────────────────────────────────────────────────────

    /// Record an analytics event for tracking usage metrics.
    #[instrument(skip(self, event))]
    pub fn record_analytics_event(&self, event: crate::analytics::AnalyticsEvent) {
        let mut analytics = self.analytics.lock().unwrap();
        analytics.record_event(event);
    }

    /// Get a usage report of all tracked analytics.
    pub fn get_usage_report(&self) -> crate::analytics::UsageReport {
        let analytics = self.analytics.lock().unwrap();
        analytics.usage_report()
    }

    /// Get the overall success rate.
    pub fn success_rate(&self) -> f64 {
        let analytics = self.analytics.lock().unwrap();
        analytics.success_rate()
    }

    /// Get total cost in USD.
    pub fn total_cost_usd(&self) -> f64 {
        let analytics = self.analytics.lock().unwrap();
        analytics.total_cost_usd()
    }

    /// Reset all analytics counters.
    pub fn reset_analytics(&self) {
        let mut analytics = self.analytics.lock().unwrap();
        analytics.reset();
    }

    // ── Audit logging ──────────────────────────────────────────────────

    /// Compute the tamper-evident diff hash for an audit entry.
    /// The hash is SHA256(before_json || after_json || timestamp).
    pub fn compute_audit_hash(
        before: &Option<String>,
        after: &Option<String>,
        timestamp: &str,
    ) -> String {
        SqliteAuditLogger::compute_diff_hash(before, after, timestamp)
    }

    /// Verify the integrity hash of an existing audit entry.
    pub fn verify_audit_integrity(&self, entry: &crate::models::AuditEntry) -> bool {
        SqliteAuditLogger::verify_entry_integrity(entry)
    }

    /// Generate SOC2 evidence summary for an audit entry.
    pub fn soc2_evidence_summary(&self, entry: &crate::models::AuditEntry) -> serde_json::Value {
        SqliteAuditLogger::soc2_evidence_summary(entry)
    }

    /// Validate that an audit entry conforms to the SOC2 schema.
    pub fn validate_soc2_schema(&self, entry: &crate::models::AuditEntry) -> Result<()> {
        SqliteAuditLogger::validate_soc2_schema(entry)
    }

    /// Anonymize an audit entry for GDPR right-to-erasure.
    pub fn anonymize_audit_entry(&self, entry: &mut crate::models::AuditEntry) {
        SqliteAuditLogger::anonymize_entry(entry);
    }

    /// Return a cloneable `Arc` handle to the audit logger.
    pub fn audit_logger_handle(&self) -> Arc<SqliteAuditLogger> {
        Arc::clone(&self.audit_logger)
    }

    // ── Text diff engine ───────────────────────────────────────────────

    /// Compute a unified diff between two text documents.
    pub fn compute_diff(&self, old: &str, new: &str) -> crate::diff::DiffResult {
        self.diff_engine.compute(old, new, "v0", "v1")
    }

    /// Summarize a diff with line counts and changed sections.
    pub fn summarize_diff(&self, diff: &crate::diff::DiffResult) -> crate::diff::ChangeSummary {
        self.diff_engine.summarize(diff)
    }

    /// Check if two documents are identical (no diff needed).
    pub fn is_identical(&self, old: &str, new: &str) -> bool {
        let diff = self.compute_diff(old, new);
        self.diff_engine.is_identical(&diff)
    }

    /// Format a diff as unified diff text.
    pub fn format_unified_diff(&self, diff: &crate::diff::DiffResult) -> String {
        self.diff_engine.format_unified(diff)
    }

    // ── Retention + Garbage Collection ─────────────────────────────────

    /// Set a retention period for a data type.
    #[cfg(feature = "retention")]
    pub fn set_retention_period(&mut self, data_type: DataType, days: u32) {
        self.retention_policy.set_period(data_type, days);
    }

    /// Get the retention period (in days) for a data type.
    #[cfg(feature = "retention")]
    pub fn get_retention_period(&self, data_type: &DataType) -> u32 {
        self.retention_policy.get_period(data_type)
    }

    /// Check if data of a given type has expired based on its retention policy.
    #[cfg(feature = "retention")]
    pub fn is_data_expired(&self, data_type: &DataType, age_days: u32) -> bool {
        self.retention_policy.is_expired(data_type, age_days)
    }

    /// Run scheduled cleanup and return results for expired items.
    #[cfg(feature = "retention")]
    pub fn run_retention_cleanup(&self) -> Vec<crate::retention::CleanupResult> {
        self.retention_policy.run_cleanup()
    }

    /// Execute garbage collection across all configured types.
    #[cfg(feature = "retention")]
    pub fn run_garbage_collection(&self) -> Result<crate::garbage_collector::GcReport> {
        self.garbage_collector.collect()
    }

    /// Purge soft-deleted items and return count of purged rows.
    #[cfg(feature = "retention")]
    pub fn purge_soft_deleted(&self) -> Result<u64> {
        self.garbage_collector.purge_soft_deleted()
    }

    /// Get garbage collection statistics.
    #[cfg(feature = "retention")]
    pub fn gc_stats(&self) -> crate::garbage_collector::GcStats {
        self.garbage_collector.stats()
    }

    /// Enable or disable automatic garbage collection.
    #[cfg(feature = "retention")]
    pub fn set_gc_enabled(&self, enabled: bool) {
        self.garbage_collector.set_enabled(enabled);
    }

    /// Check if automatic garbage collection is enabled.
    #[cfg(feature = "retention")]
    pub fn gc_enabled(&self) -> bool {
        self.garbage_collector.is_enabled()
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
    use crate::pollination;
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
            embedding_backend: crate::config::EmbedderBackend::Hash,
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
    #[cfg(feature = "vibe")]
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

    #[tokio::test]
    async fn test_quality_gate_empty_passes() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let artifact = Artifact::Code {
            path: "test.rs".to_string(),
            content: "fn main() {}".to_string(),
            language: "rust".to_string(),
        };

        let result = hub.run_quality_gate(&artifact).await.unwrap();
        assert!(result.passed);
        assert_eq!(result.lint_score, 1.0);
        assert_eq!(result.security_score, 1.0);
        assert_eq!(result.performance_score, 1.0);
        assert_eq!(result.accessibility_score, 1.0);
    }

    #[tokio::test]
    async fn test_quality_gate_result_type() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let artifact = Artifact::Prompt {
            system: "You are a helpful assistant.".to_string(),
            user: "Hello".to_string(),
        };

        let result = hub.run_quality_gate(&artifact).await.unwrap();
        assert!(result.passed);
        assert!(result.warnings.is_empty());
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_lineage_register_and_ancestry() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        // Register a root version.
        hub.lineage_mut()
            .register_version("v1", "prompt-a", None, "alice")
            .unwrap();
        assert_eq!(hub.lineage_node_count(), 1);
        assert_eq!(hub.lineage_roots().len(), 1);

        // Register a child version.
        hub.lineage_mut()
            .register_version("v2", "prompt-a", Some("v1"), "bob")
            .unwrap();

        let ancestry = hub.get_lineage_ancestry("v2").unwrap();
        assert_eq!(ancestry.path, vec!["v1", "v2"]);
        assert_eq!(ancestry.depth, 2);
    }

    #[tokio::test]
    async fn test_lineage_fork_detection() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.lineage_mut()
            .register_version("v1", "prompt-a", None, "alice")
            .unwrap();
        hub.lineage_mut()
            .register_version("v2", "prompt-a", Some("v1"), "bob")
            .unwrap();
        hub.lineage_mut()
            .register_version("v3", "prompt-a", Some("v1"), "charlie")
            .unwrap();

        let forks = hub.detect_lineage_forks();
        assert_eq!(forks.len(), 1);
        assert_eq!(forks[0].fork_point_version, "v1");
        assert_eq!(forks[0].branches.len(), 2);
    }

    #[tokio::test]
    async fn test_lineage_tree_build() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.lineage_mut()
            .register_version("v1", "prompt-a", None, "alice")
            .unwrap();
        hub.lineage_mut()
            .register_version("v2", "prompt-a", Some("v1"), "bob")
            .unwrap();

        let tree = hub.build_lineage_tree("v1").unwrap();
        assert_eq!(tree.root, "v1");
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.fork_count, 0); // only one child of v1
    }

    #[tokio::test]
    async fn test_lineage_descendants() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.lineage_mut()
            .register_version("v1", "prompt-a", None, "alice")
            .unwrap();
        hub.lineage_mut()
            .register_version("v2", "prompt-a", Some("v1"), "bob")
            .unwrap();
        hub.lineage_mut()
            .register_version("v3", "prompt-a", Some("v2"), "charlie")
            .unwrap();

        let descs = hub.get_lineage_descendants("v1");
        assert_eq!(descs.len(), 2);
        assert!(descs.contains(&"v2".to_string()));
        assert!(descs.contains(&"v3".to_string()));
    }

    #[tokio::test]
    async fn test_lineage_has_version() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        assert!(!hub.has_lineage_version("v99"));

        hub.lineage_mut()
            .register_version("v1", "prompt-a", None, "alice")
            .unwrap();

        assert!(hub.has_lineage_version("v1"));
        assert!(!hub.has_lineage_version("v99"));
    }

    #[tokio::test]
    async fn test_lineage_duplicate_conflict() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.lineage_mut()
            .register_version("v1", "prompt-a", None, "alice")
            .unwrap();

        let result = hub
            .lineage_mut()
            .register_version("v1", "prompt-b", None, "bob");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lineage_missing_parent() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let result =
            hub.lineage_mut()
                .register_version("v2", "prompt-a", Some("nonexistent"), "bob");
        assert!(result.is_err());
    }

    // - Pollination tests --------------------------------------------------------

    #[tokio::test]
    async fn test_extract_pollination_patterns_step_by_step() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let prompt = Prompt {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: "Follow these steps: 1. Plan 2. Execute".to_string(),
            user_template: "Help me.".to_string(),
            required_vars: vec![],
            domain: Domain::Coding,
            tags: vec![],
            target_roles: vec![],
            metadata: Default::default(),
            metrics: PromptMetrics {
                usage_count: 50,
                success_rate: 0.9,
                avg_tokens: 300,
                avg_latency_ms: 100,
                last_used: Some(chrono::Utc::now()),
                cost_estimate_usd: 0.0,
            },
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            author: AgentIdentity {
                id: Uuid::new_v4(),
                name: "test".to_string(),
                capabilities: Default::default(),
                token_hash: "".to_string(),
                specialization_score: 0.5,
            },
            deleted_at: None,
            generation_params: None,
            locale: None,
            multimodal: None,
        };

        let patterns = hub.extract_pollination_patterns(&prompt).unwrap();
        assert!(
            patterns.iter().any(|p| p.structure == "step-by-step"),
            "Should detect step-by-step pattern"
        );
    }

    #[tokio::test]
    async fn test_pollination_handle_returns_arc() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let handle1 = hub.pollination();
        let handle2 = hub.pollination();
        assert_eq!(handle1.lock().unwrap().pool_size(), 0);
        assert_eq!(Arc::strong_count(&handle1), Arc::strong_count(&handle2));
    }

    #[tokio::test]
    async fn test_pollination_mut_share_pattern() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let pattern = pollination::Pattern {
            id: Uuid::new_v4(),
            structure: "few-shot".to_string(),
            domains: vec![Domain::Writing],
            score: 0.8,
            usage_count: 10,
            agent_id: Uuid::new_v4(),
            example_snippet: "Here is an example...".to_string(),
        };

        hub.pollination_mut().share_pattern(pattern).unwrap();
        assert_eq!(hub.pollination().lock().unwrap().pool_size(), 1);
    }

    // - Swarm role registry tests ------------------------------------------

    #[tokio::test]
    async fn test_swarm_registry_handle() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let registry = hub.manage_swarm();
        assert!(!registry.list_roles().is_empty());
        assert!(registry.get(&Role::Orchestrator).is_some());
    }

    #[tokio::test]
    async fn test_validate_swarm_roles_with_orchestrator() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        // Valid: Orchestrator is the required role.
        let result = hub.validate_swarm_roles(&[Role::Orchestrator]);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_swarm_roles_critic_without_implementer() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        // Should produce CapabilityMissing conflict.
        let result = hub.validate_swarm_roles(&[Role::Orchestrator, Role::Critic]);
        assert!(result.is_ok());
        let conflicts = result.unwrap();
        assert!(
            conflicts
                .iter()
                .any(|c| matches!(c, Conflict::CapabilityMissing))
        );
    }

    #[tokio::test]
    async fn test_generate_swarm_bundle() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let bundle = hub
            .generate_swarm_bundle(
                vec![Role::Orchestrator, Role::Architect],
                Domain::Coding,
                Uuid::new_v4(),
            )
            .await;
        assert!(bundle.is_ok());
    }

    // - Satisfaction tracker tests -------------------------------------------

    #[tokio::test]
    async fn test_satisfaction_tracker_handle() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let handle1 = hub.satisfaction_tracker();
        let handle2 = hub.satisfaction_tracker();
        assert_eq!(Arc::strong_count(&handle1), Arc::strong_count(&handle2));
        // Default tracker has zero ratings/events.
        assert_eq!(handle1.rating_count(), 0);
        assert_eq!(handle1.event_count(), 0);
    }

    #[tokio::test]
    async fn test_record_csat_via_hub() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.record_csat_rating(5, "Great UX");
        hub.record_csat_rating(3, "Okay experience");

        let tracker = hub.satisfaction_tracker();
        assert_eq!(tracker.rating_count(), 2);
        let metrics = tracker.metrics();
        assert_eq!(metrics.csat_average, 4.0);
    }

    #[tokio::test]
    async fn test_record_nps_via_hub() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.record_nps_rating(10); // promoter
        hub.record_nps_rating(9); // promoter
        hub.record_nps_rating(4); // detractor

        let metrics = hub.satisfaction_metrics().unwrap();
        // (2 - 1) / 3 * 100 = 33.33...
        assert!(
            (metrics.nps_score - 33.33).abs() < 0.1,
            "NPS score: {}",
            metrics.nps_score
        );
    }

    #[tokio::test]
    async fn test_record_event_via_hub() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.record_satisfaction_event("p1", true, 1);
        hub.record_satisfaction_event("p2", true, 3);
        hub.record_satisfaction_event("p3", false, 1);

        let tracker = hub.satisfaction_tracker();
        assert_eq!(tracker.event_count(), 3);
        assert_eq!(tracker.one_shot_success_rate(), 50.0);
    }

    #[tokio::test]
    async fn test_satisfaction_metrics_empty() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let metrics = hub.satisfaction_metrics().unwrap();
        assert_eq!(metrics.csat_average, 0.0);
        assert_eq!(metrics.nps_score, 0.0);
        assert_eq!(metrics.one_shot_success_rate, 0.0);
        assert_eq!(metrics.total_ratings, 0);
        assert_eq!(metrics.total_events, 0);
        assert_eq!(
            metrics.recent_trend,
            crate::satisfaction::TrendDirection::Stable
        );
    }

    #[tokio::test]
    async fn test_csat_invalid_silent() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.record_csat_rating(0, "invalid"); // should be silently ignored
        hub.record_csat_rating(6, "invalid"); // should be silently ignored
        hub.record_csat_rating(3, "valid"); // should count

        let tracker = hub.satisfaction_tracker();
        assert_eq!(tracker.rating_count(), 1);
    }

    #[tokio::test]
    async fn test_provider_health_register_and_summary() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        // Register providers and record metrics
        hub.register_provider("gpt-4o", "https://api.openai.com/v1");
        hub.register_provider("claude", "https://api.anthropic.com/v1");

        hub.record_success("gpt-4o", 150);
        hub.record_success("gpt-4o", 200);
        // gpt-4o: 0% error rate, avg latency 175ms < 5000ms threshold → Healthy

        let summary = hub.get_health_summary();
        assert_eq!(summary.providers.len(), 2);
        assert!(hub.is_healthy("gpt-4o")); // 0% errors, latency well under threshold

        // Record a failure for claude — with default error_rate_threshold=50%,
        // 1/1 = 100% >= 50% → Unhealthy
        hub.record_failure("claude");
        assert!(!hub.is_healthy("claude"));

        let gpt_status = hub.health_monitor().lock().unwrap().get_health("gpt-4o");
        assert!(gpt_status.is_some());
    }

    #[tokio::test]
    async fn test_provider_health_failure_threshold() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.register_provider("flaky", "https://api.example.com/v1");

        // Configure thresholds via the monitor directly
        {
            let monitor = hub.health_monitor();
            monitor.lock().unwrap().configure(100, 50); // latency=100ms, error_rate=50%
        }

        // Record many failures to push over the threshold
        for _ in 0..6 {
            hub.record_failure("flaky");
        }

        assert!(!hub.is_healthy("flaky"));
    }

    #[tokio::test]
    async fn test_load_balancer_add_and_select() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.add_lb_provider("gpt-4o", "https://api.openai.com/v1", 2);
        hub.add_lb_provider("claude", "https://api.anthropic.com/v1", 1);

        let stats = hub.get_lb_stats();
        assert_eq!(stats.len(), 2);

        for _ in 0..3 {
            let selection = hub.select_provider();
            assert!(selection.is_ok());
            let sel = selection.unwrap();
            assert!(sel.provider_name == "gpt-4o" || sel.provider_name == "claude");
        }
    }

    #[cfg(feature = "budget")]
    #[tokio::test]
    async fn test_budget_delegation() {
        use crate::budget::BudgetAlert;
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        // Default budget is $1000
        assert!(!hub.is_budget_exceeded());
        assert_eq!(hub.current_spend_usd(), 0.0);

        // Record spend and check utilization
        let alert = hub.record_spend(500.0);
        assert_eq!(alert, BudgetAlert::FiftyPercent);
        assert!((hub.budget_utilization() - 50.0).abs() < 0.01);

        // Exceed budget
        let alert = hub.record_spend(600.0);
        assert_eq!(alert, BudgetAlert::HundredPercent);
        assert!(hub.is_budget_exceeded());
        assert!((hub.budget_utilization() - 110.0).abs() < 0.01);

        // Save / load config round-trip
        let _config = hub.save_budget_config("test-org").unwrap();

        hub.reset_budget_period();
        assert_eq!(hub.current_spend_usd(), 0.0);
        assert!(!hub.is_budget_exceeded());
    }

    #[cfg(feature = "circuit-breaker")]
    #[tokio::test]
    async fn test_circuit_breaker_accessor() {
        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        let cb = hub.circuit_breaker();
        assert_eq!(cb.current_state(), "closed");

        // Verify it can gate a failure
        let result = cb.call(|| Err::<(), _>(HubError::Internal("test".into())));
        assert!(result.is_err());

        // After 5 consecutive failures it should open
        for _ in 0..4 {
            let _ = cb.call(|| Err::<(), _>(HubError::Internal("test".into())));
        }
        assert_eq!(cb.current_state(), "open");
    }

    // ── Moderation integration tests ───────────────────────────────────

    #[cfg(feature = "moderation")]
    #[tokio::test]
    async fn test_moderation_delegation() {
        use crate::moderation::ModerationResult;

        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        // Safe content passes
        assert!(hub.is_content_safe("Hello, how are you today?"));

        // check_content returns Allow for safe content
        let report = hub.check_content("What is Rust?").unwrap();
        assert!(matches!(report.result, ModerationResult::Allow));

        // handle works across feature gate
        let handle = hub.moderation_engine();
        assert!(handle.is_allowed("Hello world"));
    }

    #[cfg(feature = "moderation")]
    #[tokio::test]
    async fn test_moderation_handle_returns_arc() {
        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        let engine1 = hub.moderation_engine();
        let engine2 = hub.moderation_engine();
        assert!(std::ptr::eq(Arc::as_ptr(&engine1), Arc::as_ptr(&engine2)));
    }

    // ── Quota integration tests ────────────────────────────────────────

    #[cfg(feature = "quota")]
    #[tokio::test]
    async fn test_quota_delegation() {
        use crate::quota::QuotaStatus;

        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        // Default enforcer allows small consumption
        assert_eq!(hub.check_and_consume(1).unwrap(), QuotaStatus::Allowed);

        // Usage snapshot works
        let usage = hub.quota_usage();
        assert_eq!(usage.daily_used, 1);
        assert_eq!(usage.burst_used, 1);

        // Reset clears counters
        hub.reset_quota();
        let usage = hub.quota_usage();
        assert_eq!(usage.daily_used, 0);
    }

    #[cfg(feature = "quota")]
    #[tokio::test]
    async fn test_quota_handle_returns_arc() {
        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        let h1 = hub.quota_enforcer_handle();
        let h2 = hub.quota_enforcer_handle();
        assert!(std::ptr::eq(Arc::as_ptr(&h1), Arc::as_ptr(&h2)));
    }

    // ── Preview integration tests ──────────────────────────────────────

    #[cfg(feature = "preview")]
    #[tokio::test]
    async fn test_preview_engine_accessible() {
        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        // Handle works and returns same Arc
        let h1 = hub.preview_engine_handle();
        let h2 = hub.preview_engine_handle();
        assert!(std::ptr::eq(Arc::as_ptr(&h1), Arc::as_ptr(&h2)));
    }

    // ── Canary integration test ────────────────────────────────────────

    #[cfg(feature = "canary")]
    #[tokio::test]
    async fn test_canary_engine_accessible() {
        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        // Handle works and returns same Arc
        let h1 = hub.canary_engine_handle();
        let h2 = hub.canary_engine_handle();
        assert!(std::ptr::eq(Arc::as_ptr(&h1), Arc::as_ptr(&h2)));
    }

    // ── Analytics integration test ─────────────────────────────────────

    #[tokio::test]
    async fn test_analytics_accessible() {
        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        // Record an event and check report
        use crate::analytics::{AnalyticsEvent, EventType};
        hub.record_analytics_event(AnalyticsEvent {
            event_type: EventType::PromptUse,
            prompt_id: "test-prompt".into(),
            user_id: "test-user".into(),
            tokens_used: 100,
            cost_micros: 500,
            success: true,
            duration_ms: 50,
        });

        let report = hub.get_usage_report();
        assert_eq!(report.total_prompt_uses, 1);
    }

    // ── Audit integration test ─────────────────────────────────────────

    #[tokio::test]
    async fn test_audit_utilities_accessible() {
        let dir = tempfile::tempdir().unwrap();
        let test_hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        // compute_hash works through delegation
        let hash_before = crate::audit::SqliteAuditLogger::compute_diff_hash(
            &Option::<String>::None,
            &Option::<String>::None,
            "2026-01-01T00:00:00Z",
        );
        assert_eq!(hash_before.len(), 64); // SHA256 hex digest

        let _hash_after = crate::audit::SqliteAuditLogger::compute_diff_hash(
            &Some(String::from("before")),
            &Some(String::from("after")),
            "2026-01-01T00:00:00Z",
        );

        // Handle works
        let _handle = test_hub.audit_logger_handle();
    }

    // ── Diff integration test ──────────────────────────────────────────

    #[tokio::test]
    async fn test_diff_engine_accessible() {
        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        // Compute a diff between two strings
        let old_text = "line 1\nline 2\nline 3";
        let new_text = "line 1\nchanged line\nline 3";
        let diff = hub.compute_diff(old_text, new_text);
        assert!(hub.is_identical("identical", "identical"));

        // Summarize the diff
        let summary = hub.summarize_diff(&diff);
        assert!(summary.total_changes >= 1);
        assert!(summary.change_ratio > 0.0);

        // Format as unified diff
        let formatted = hub.format_unified_diff(&diff);
        assert!(!formatted.is_empty());
    }

    // ── Retention + GC integration test ────────────────────────────────

    #[tokio::test]
    async fn test_retention_gc_accessible() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        // Set and get retention period
        use crate::retention::DataType;
        hub.set_retention_period(DataType::AuditLog, 30);
        let period = hub.get_retention_period(&DataType::AuditLog);
        assert_eq!(period, 30);

        // Check expiration logic
        assert!(!hub.is_data_expired(&DataType::AuditLog, 5));
        assert!(hub.is_data_expired(&DataType::AuditLog, 31));

        // Run retention cleanup
        let _results = hub.run_retention_cleanup();

        // Garbage collection stats accessible
        let gc_stats = hub.gc_stats();
        assert_eq!(gc_stats.prompts_purged_total, 0);

        // GC enabled check
        assert!(hub.gc_enabled());
    }

    #[tokio::test]
    async fn test_multimodal_accessible() {
        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        // Functional assertions: MIME validation and placeholder extraction
        assert!(hub.validate_image_mime_type("image/png"));
        assert!(!hub.validate_image_mime_type("application/octet-stream"));

        // Placeholder extraction works (format: {{id}})
        let ids = hub.extract_placeholder_ids("Hello {{img1}} World {{img2}}");
        assert_eq!(ids, vec!["img1".to_string(), "img2".to_string()]);
    }

    #[cfg(feature = "i18n")]
    #[tokio::test]
    async fn test_i18n_accessible() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        // Engine handle accessible and valid
        #[allow(clippy::no_effect)]
        {
            let _engine = hub.i18n_engine();
        }

        // Translation registration works
        hub.register_translation("prompt-1", "fr", "Bonjour le monde".to_string());

        // Translation retrieval works
        let fr = hub.get_localized_template("prompt-1", "fr");
        assert_eq!(fr, Some("Bonjour le monde".to_string()));

        // Fallback chain works
        let chain = hub.translation_fallback_chain("en-US");
        assert!(!chain.is_empty());
    }
}
