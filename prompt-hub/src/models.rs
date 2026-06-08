#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

// ─────────────────────────────────────────────
// Core domain enums
// ─────────────────────────────────────────────

/// Status of a prompt in the hub
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum, Default)]
pub enum Status {
    #[default]
    Draft,
    Active,
    Deprecated,
    Archived,
    Locked,
}

/// Domain classification for prompts
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum, Default,
)]
pub enum Domain {
    Coding,
    DevOps,
    Security,
    Analysis,
    Design,
    DataScience,
    Testing,
    Documentation,
    Writing,
    #[default]
    General,
}

/// Role classification for intent.
///
/// Note: this enum carries a data-bearing `Custom(String)` variant, so it
/// intentionally does NOT derive `clap::ValueEnum` (which requires all-unit
/// variants) nor `Copy`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Role {
    Architect,
    #[default]
    Developer,
    Tester,
    DevOps,
    Analyst,
    Designer,
    Orchestrator,
    Reviewer,
    Implementer,
    Refiner,
    Critic,
    Junie,
    Custom(String),
}

/// Type of task the user wants to perform
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskType {
    Create,
    Fix,
    Improve,
    Explain,
    Convert,
    Test,
    Deploy,
    Review,
}

/// Complexity level of the request
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Complexity {
    Simple,
    Moderate,
    Complex,
    Research,
}

/// Urgency level
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Urgency {
    Low,
    Medium,
    High,
    Critical,
}

/// User skill level for tailoring output
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillLevel {
    Beginner,
    Intermediate,
    Expert,
}

/// Evolution strategy for prompt improvement
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum, Default)]
pub enum EvolutionStrategy {
    /// Mutate single prompt parameters
    #[default]
    Mutate,
    /// Crossover between two parent prompts
    Crossover,
    /// A/B test variant generation
    AbTest,
    /// Semantic variation
    Semantic,
    /// Compress prompt while preserving meaning
    Compress,
    /// Expand prompt with more detail
    Expand,
}

// ─────────────────────────────────────────────
// Identity and metadata types
// ─────────────────────────────────────────────

/// Identity of an agent in the swarm.
///
/// Holds a floating-point `specialization_score`, so it cannot derive
/// `Eq`/`Hash`; it is never used as a map key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentIdentity {
    pub id: Uuid,
    pub name: String,
    pub capabilities: Vec<Capability>,
    pub token_hash: String,
    pub specialization_score: f64,
}

impl Default for AgentIdentity {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "anonymous".to_string(),
            capabilities: Vec::new(),
            token_hash: String::new(),
            specialization_score: 0.0,
        }
    }
}

impl AgentIdentity {
    /// Identity for a **trusted local operator** — the single user who owns the
    /// local prompt store (e.g. the `prompthub` CLI acting on its on-disk DB).
    ///
    /// Unlike [`AgentIdentity::default`] (the capability-less `anonymous`
    /// identity), this grants the full set of capabilities a local owner needs
    /// to use every command out of the box: [`Capability::Read`],
    /// [`Capability::Write`], and [`Capability::Admin`] (which together cover
    /// every [`Action`](crate::auth::Action) — read, write/lock/evolve, and
    /// delete/admin/transfer).
    ///
    /// This does **not** weaken RBAC: [`RbacAuthManager`](crate::auth::RbacAuthManager)
    /// is still the enforcement point and still rejects capability-less or
    /// remote/swarm identities. This constructor merely packages the grant a
    /// local DB owner legitimately holds; swarm/automation/remote callers must
    /// continue to present their own explicitly-granted capabilities.
    pub fn local_operator(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            capabilities: vec![Capability::Read, Capability::Write, Capability::Admin],
            token_hash: String::new(),
            specialization_score: 0.0,
        }
    }
}

/// Metadata associated with a prompt.
///
/// Holds a floating-point `success_rate` and a `HashMap`, so it cannot derive
/// `Eq`/`Hash`; it is never used as a map key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptMeta {
    pub description: Option<String>,
    pub usage_count: u64,
    pub success_rate: f64,
    pub avg_latency_ms: u64,
    pub last_used: Option<DateTime<Utc>>,
    pub custom_properties: HashMap<String, String>,
}

impl Default for PromptMeta {
    fn default() -> Self {
        Self {
            description: None,
            usage_count: 0,
            success_rate: 0.0,
            avg_latency_ms: 0,
            last_used: None,
            custom_properties: HashMap::new(),
        }
    }
}

/// Metrics for prompt performance.
///
/// Shaped to match the `metrics` table (migration 0001):
/// `usage_count, success_rate, avg_tokens, avg_latency_ms, last_used, cost_estimate_usd`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptMetrics {
    pub usage_count: u64,
    pub success_rate: f64,
    pub avg_tokens: u64,
    pub avg_latency_ms: u64,
    pub last_used: Option<DateTime<Utc>>,
    pub cost_estimate_usd: f64,
}

impl Default for PromptMetrics {
    fn default() -> Self {
        Self {
            usage_count: 0,
            success_rate: 0.0,
            avg_tokens: 0,
            avg_latency_ms: 0,
            last_used: None,
            cost_estimate_usd: 0.0,
        }
    }
}

/// Generation parameters for LLM calls
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationParams {
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: Option<u32>,
    pub stop_sequences: Vec<String>,
    pub frequency_penalty: f64,
    pub presence_penalty: f64,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 1.0,
            max_tokens: None,
            stop_sequences: Vec::new(),
            frequency_penalty: 0.0,
            presence_penalty: 0.0,
        }
    }
}

/// Multi-modal support configuration
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct MultimodalConfig {
    pub supports_images: bool,
    pub supports_audio: bool,
    pub supports_video: bool,
    pub image_placeholders: Vec<String>,
}

// ─────────────────────────────────────────────
// Missing types referenced across codebase
// ─────────────────────────────────────────────

/// Search mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SearchMode {
    Fast,
    Smart,
    Hybrid,
}

/// Agent capability for RBAC
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    Read,
    Write,
    Admin,
    Lock,
    SwarmOnly,
    Automation,
    Execute,
}

/// Health status for plugins, providers, and subsystem checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    #[default]
    Unknown,
}

/// Conflict types for swarm validation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Conflict {
    MissingRole,
    DuplicateRole(Role),
    CapabilityMissing,
    Custom(String),
    CircularDependency,
    DomainMismatch,
}

/// Privacy issue types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyIssue {
    Secret { key: String },
    Pii { type_: String },
}

/// Privacy scan report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyReport {
    pub issues: Vec<PrivacyIssue>,
    pub sanitized: bool,
    pub secrets_found: usize,
    pub pii_found: usize,
    pub risk_level: String,
}

/// Search filters for prompt queries
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchFilters {
    pub role: Option<Role>,
    pub domain: Option<Domain>,
    pub tags: Vec<String>,
    pub status: Option<Status>,
}

/// Pagination parameters
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Pagination {
    pub page: usize,
    pub per_page: usize,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
        }
    }
}

/// Generic paginated results wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paginated<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
}

/// Scored prompt result from search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredPrompt {
    pub prompt: Prompt,
    pub score: f32,
    pub matched_field: Option<String>,
}

// ─────────────────────────────────────────────
// Core Prompt type
// ─────────────────────────────────────────────

/// The central prompt entity managed by PromptHub.
///
/// This is the primary data type stored, versioned, searched, and served.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub id: Uuid,
    pub name: String,
    pub version: semver::Version,
    pub status: Status,
    pub system_prompt: String,
    pub user_template: String,
    pub required_vars: Vec<String>,
    pub domain: Domain,
    pub tags: Vec<String>,
    pub target_roles: Vec<Role>,
    pub metadata: PromptMeta,
    pub metrics: PromptMetrics,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub author: AgentIdentity,
    pub deleted_at: Option<DateTime<Utc>>,
    pub generation_params: Option<GenerationParams>,
    pub locale: Option<String>,
    pub multimodal: Option<MultimodalConfig>,
}

impl Default for Prompt {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "untitled".to_string(),
            version: semver::Version::new(0, 1, 0),
            status: Status::default(),
            system_prompt: String::new(),
            user_template: String::new(),
            required_vars: Vec::new(),
            domain: Domain::default(),
            tags: Vec::new(),
            target_roles: Vec::new(),
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
}

impl Prompt {
    /// Create a minimal prompt with just a name and system prompt.
    /// All other fields use sensible defaults.
    pub fn new(name: &str, system_prompt: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            version: semver::Version::new(0, 1, 0),
            status: Status::Draft,
            system_prompt: system_prompt.to_string(),
            user_template: String::new(),
            required_vars: Vec::new(),
            domain: Domain::General,
            tags: Vec::new(),
            target_roles: Vec::new(),
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
}

/// A recorded version snapshot of a prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionRecord {
    pub id: i64,
    pub prompt_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub version: semver::Version,
    pub changelog: String,
    pub diff: String,
    pub created_at: DateTime<Utc>,
}

// ─────────────────────────────────────────────
// Supporting types
// ─────────────────────────────────────────────

/// Lock token for editing a prompt.
///
/// Shaped to match the `locks` table (migration 0003):
/// `id, prompt_id, agent_id, token_hash, expires_at, created_at`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockToken {
    /// Lock row id (primary key).
    pub id: Uuid,
    pub prompt_id: Uuid,
    /// Id of the agent holding the lock (compared against `AgentIdentity::id`).
    pub agent_id: Uuid,
    /// Hash of the opaque lock token handed back to the holder.
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Audit entry for tracking changes.
///
/// Shaped to match the `audit_log` table (migration 0002):
/// `id, timestamp, agent_id, action, prompt_id, diff_hash, before_json, after_json, ip_address`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Autoincrement row id.
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    /// Id of the agent that performed the action.
    pub agent_id: Uuid,
    /// Action name (stored as free-form text in the audit log).
    pub action: String,
    pub prompt_id: Option<Uuid>,
    /// Hash of the before→after diff for tamper-evidence.
    pub diff_hash: String,
    pub before_json: Option<String>,
    pub after_json: Option<String>,
    pub ip_address: Option<String>,
}

/// Type of audit action
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuditAction {
    Created,
    Updated,
    RolledBack,
    Deleted,
    Locked,
    Unlocked,
    Exported,
    Imported,
    Evolved,
    Deployed,
    Reviewed,
}

/// Swarm bundle for role-based prompt distribution.
///
/// A bundle ties a workflow to a set of per-role prompts plus the consistency
/// and evolution analysis produced when the bundle was generated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmBundle {
    /// Workflow this bundle belongs to.
    pub workflow_id: Uuid,
    /// Prompt text keyed by role.
    pub role_prompts: HashMap<Role, String>,
    /// Standardized handoff template between roles.
    pub handoff_template: String,
    /// Conflicts detected during consistency checking.
    pub consistency_report: Vec<Conflict>,
    /// Suggested evolution actions for this bundle.
    pub evolution_suggestions: Vec<String>,
}

// ─────────────────────────────────────────────
// Execution and result types
// ─────────────────────────────────────────────

/// Classified user intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub raw_text: String,
    pub domain: Domain,
    pub role: Role,
    pub task_type: TaskType,
    pub complexity: Complexity,
    pub urgency: Urgency,
    pub extracted_entities: HashMap<String, String>,
}

impl Default for Intent {
    fn default() -> Self {
        Self {
            raw_text: String::new(),
            domain: Domain::General,
            role: Role::Developer,
            task_type: TaskType::Create,
            complexity: Complexity::Simple,
            urgency: Urgency::Medium,
            extracted_entities: HashMap::new(),
        }
    }
}

/// Modality of a piece of user input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum InputType {
    #[default]
    Text,
    Voice,
    Screenshot,
    Sketch,
    File,
    Url,
}

/// A single multimodal user input.
///
/// Carries the raw bytes (`raw_data`) plus a normalized text projection
/// (`extracted_text`) used by intent classification and privacy scanning.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserInput {
    pub input_type: InputType,
    pub raw_data: Vec<u8>,
    pub extracted_text: String,
}

/// Uploaded file data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileData {
    pub name: String,
    pub content: Vec<u8>,
    pub mime_type: String,
}

/// Project context gathered from filesystem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContext {
    /// Filesystem path the context was gathered from.
    pub project_path: String,
    pub language: String,
    pub framework: String,
    pub database: Option<String>,
    pub styling: Option<String>,
    pub auth: Option<String>,
    pub existing_files: Vec<FileEntry>,
    pub environment_variables: HashMap<String, String>,
    pub team_size: usize,
}

impl Default for ProjectContext {
    fn default() -> Self {
        Self {
            project_path: String::new(),
            language: "unknown".to_string(),
            framework: "unknown".to_string(),
            database: None,
            styling: None,
            auth: None,
            existing_files: Vec::new(),
            environment_variables: HashMap::new(),
            team_size: 1,
        }
    }
}

/// File entry in project context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub modified: DateTime<Utc>,
}

/// Generated artifact from the engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Artifact {
    Prompt {
        system: String,
        user: String,
    },
    Code {
        path: String,
        content: String,
        language: String,
    },
    Config {
        path: String,
        content: String,
        format: String,
    },
    Test {
        path: String,
        content: String,
        framework: String,
    },
    Migration {
        path: String,
        content: String,
        database: String,
    },
    Documentation {
        title: String,
        content: String,
        format: String,
    },
}

/// Execution plan step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub id: usize,
    pub description: String,
    pub action: String,
    pub dependencies: Vec<usize>,
    pub estimated_duration_secs: u64,
}

/// Full execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub title: String,
    pub description: String,
    pub steps: Vec<ExecutionStep>,
    pub total_estimated_duration_secs: u64,
}

impl Default for ExecutionPlan {
    fn default() -> Self {
        Self {
            title: "Default Plan".to_string(),
            description: "Auto-generated plan".to_string(),
            steps: Vec::new(),
            total_estimated_duration_secs: 0,
        }
    }
}

/// Execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub actions: Vec<String>,
    pub reasoning: String,
    pub files_changed: Vec<String>,
    pub next_suggestions: Vec<String>,
    pub duration: Duration,
    pub token_cost: f64,
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self {
            success: true,
            actions: Vec::new(),
            reasoning: String::new(),
            files_changed: Vec::new(),
            next_suggestions: Vec::new(),
            duration: Duration::ZERO,
            token_cost: 0.0,
        }
    }
}

/// Final result from vibe coding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VibeResult {
    pub artifacts: Vec<Artifact>,
    pub summary: String,
    pub next_suggestions: Vec<String>,
    pub cost_estimate: CostEstimate,
    pub confidence: f64,
    pub execution_time_ms: u64,
}

/// Cost estimate for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub cost_usd: f64,
    /// Alias surfaced to callers that report the estimated dollar cost.
    pub estimated_cost_usd: f64,
    pub time_seconds: u32,
    pub confidence: f64,
}

impl Default for CostEstimate {
    fn default() -> Self {
        Self {
            tokens_input: 0,
            tokens_output: 0,
            cost_usd: 0.0,
            estimated_cost_usd: 0.0,
            time_seconds: 0,
            confidence: 0.0,
        }
    }
}

/// Confidence score breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceScore {
    pub intent_clarity: f64,
    pub context_completeness: f64,
    pub skill_match: f64,
    pub historical_success: f64,
    pub overall: f64,
    /// Aggregate confidence surfaced to callers (mirrors `overall`).
    pub score: f64,
    pub requires_confirmation: bool,
}

impl Default for ConfidenceScore {
    fn default() -> Self {
        Self {
            intent_clarity: 0.5,
            context_completeness: 0.5,
            skill_match: 0.5,
            historical_success: 0.5,
            overall: 0.5,
            score: 0.5,
            requires_confirmation: true,
        }
    }
}

/// Preview file for code previews
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewFile {
    pub path: String,
    pub content: String,
    pub language: String,
}

// ─────────────────────────────────────────────
// Spec-required types
// ─────────────────────────────────────────────

/// Standalone prompt version record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVersion {
    pub id: Uuid,
    pub prompt_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub version: semver::Version,
    pub changelog: String,
    pub diff: String,
    pub created_at: DateTime<Utc>,
}

/// Budget manager for cost control
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetManager {
    pub user_id: Uuid,
    pub monthly_budget_usd: f64,
    pub current_month_spend: f64,
    pub alert_threshold: f64,
    pub hard_limit: bool,
    pub reset_date: DateTime<Utc>,
}

/// Cost limiter per request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostLimiter {
    pub max_tokens_per_request: u32,
    pub max_cost_per_request_usd: f64,
    pub max_execution_time_seconds: u32,
    pub circuit_breaker_threshold: u32,
}

/// Token quota tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenQuota {
    pub user_id: Uuid,
    pub daily_quota: u32,
    pub hourly_quota: u32,
    pub burst_quota: u32,
    pub current_hour_usage: u32,
    pub current_day_usage: u32,
}

/// LLM provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMProvider {
    pub name: String,
    pub api_key_env: String,
    pub base_url: String,
    pub model: String,
    pub priority: u8,
    pub cost_per_1k_tokens: f64,
    pub max_tokens: u32,
    pub timeout_seconds: u32,
}

/// Local inference providers we can target.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocalProviderKind {
    Ollama,
    Llamafile,
    WhisperCPP,
}

impl std::fmt::Display for LocalProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocalProviderKind::Ollama => write!(f, "ollama"),
            LocalProviderKind::Llamafile => write!(f, "llamafile"),
            LocalProviderKind::WhisperCPP => write!(f, "whisper-cpp"),
        }
    }
}

/// Configuration for a single local model endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModelConfig {
    /// Provider type (determines API protocol).
    pub provider: LocalProviderKind,
    /// Base URL of the local inference server
    /// (e.g. "http://localhost:11434", "http://localhost:8081/v1").
    pub base_url: String,
    /// Model identifier as understood by the provider
    /// (e.g. "llama3.2", "mistral-nemo:latest").
    pub model_name: String,
    /// Sampling temperature [0.0, 2.0].
    pub temperature: f32,
    /// Top-p sampling threshold.
    pub top_p: f32,
    /// Maximum number of tokens in the response.
    pub max_tokens: u32,
}

impl LocalModelConfig {
    pub fn new(provider: LocalProviderKind, base_url: &str, model_name: &str) -> Self {
        Self {
            provider,
            base_url: base_url.to_string(),
            model_name: model_name.to_string(),
            temperature: 0.7,
            top_p: 0.9,
            max_tokens: 2048,
        }
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = top_p;
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

/// Health status of a local model instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalModelHealth {
    Healthy,
    Degraded,
    Unavailable,
}

impl LocalModelHealth {
    pub fn is_healthy(&self) -> bool {
        matches!(self, LocalModelHealth::Healthy)
    }
}

/// Information about a model available on a local endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub format: String,
    pub size_bytes: u64,
    pub status: LocalModelHealth,
    pub downloaded_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Canary deployment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanaryDeployment {
    pub feature: String,
    pub canary_percentage: f64,
    pub target_users: Vec<Uuid>,
    pub rollback_threshold: f64,
}

/// Staged rollout stages for gradual deployment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RolloutStage {
    Internal,   // no external users
    Alpha(u8),  // up to p% of total traffic
    Beta50(u8), // up to 50% + p% variance
    Beta90(u8), // up to 90% + p% variance
    Production, // full production
}

/// A rollout segment (A/B/n variant) in a graduated rollout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutSegment {
    pub name: String,
    pub percentage: u8,
    pub target_users: Vec<Uuid>,
    pub rollout_stage: RolloutStage,
    pub created_at: DateTime<Utc>,
}

/// Auto-rollback policy for canary deployments.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AutoRollbackPolicy {
    OnErrorRate {
        threshold: f64,
    },
    OnLatencyP99 {
        sla_ms: u64,
    },
    OnBoth {
        error_rate: f64,
        latency_p99_ms: u64,
    },
}

/// A canary/graduated rollout configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraduatedRolloutConfig {
    pub rollout_id: String,
    pub feature: String,
    pub segments: Vec<RolloutSegment>,
    pub auto_rollback: AutoRollbackPolicy,
    pub active: bool,
}

/// Alias for backward compat.
#[deprecated(note = "Use GraduatedRolloutConfig; CanaryDeployment is kept for canary_deploy() API")]
pub type CanaryConfig = GraduatedRolloutConfig;

/// User profile for personalization and progressive disclosure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: Uuid,
    pub display_name: String,
    pub skill_level: SkillLevel,
    pub preferred_domain: Option<Domain>,
    pub preferred_role: Option<Role>,
    pub history: Vec<UserHistoryEntry>,
    pub preferences: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Single entry in user history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserHistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub prompt_id: Option<Uuid>,
    pub result_summary: String,
}

impl Default for UserProfile {
    fn default() -> Self {
        Self {
            user_id: Uuid::new_v4(),
            display_name: "User".to_string(),
            skill_level: SkillLevel::Beginner,
            preferred_domain: None,
            preferred_role: None,
            history: Vec::new(),
            preferences: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

/// User correction for learning engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCorrection {
    pub original_intent: String,
    /// The corrected/expected output the user supplied.
    pub corrected_output: String,
    /// Free-form feedback explaining the correction.
    pub feedback: String,
    pub agent_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

/// Patch for updating an existing prompt
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptPatch {
    pub name: Option<String>,
    pub system_prompt: Option<String>,
    pub user_template: Option<String>,
    pub required_vars: Option<Vec<String>>,
    pub domain: Option<Domain>,
    pub tags: Option<Vec<String>>,
    pub target_roles: Option<Vec<Role>>,
    pub status: Option<Status>,
    pub metadata: Option<PromptMeta>,
    pub generation_params: Option<GenerationParams>,
    pub locale: Option<String>,
}

/// Sandbox mode: what level of restriction applies to this prompt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SandboxMode {
    /// No sandboxing — full access.
    Unrestricted,
    /// Resource-bounded execution with configurable limits.
    Bounded(SandboxConfig),
    /// Full isolation: deny all network, strict token/cost caps.
    Isolated(SandboxConfig),
}

#[allow(clippy::derivable_impls)]
impl Default for SandboxMode {
    fn default() -> Self {
        Self::Unrestricted
    }
}

/// Resource limits enforced by the sandbox engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub max_tokens: u32,
    pub max_cost_usd: f64,
    pub timeout_secs: u64,
    pub max_memory_bytes: u64,
    pub rate_limit_per_min: u32,
    pub deny_network: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_tokens: 8192,
            max_cost_usd: 10.0,
            timeout_secs: 60,
            max_memory_bytes: 268_435_456, // 256 MB
            rate_limit_per_min: 60,
            deny_network: true,
        }
    }
}

/// A named sandbox instance bound to a prompt or user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sandbox {
    pub id: Uuid,
    pub name: String,
    pub mode: SandboxMode,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "default".to_string(),
            mode: SandboxMode::default(),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

/// Result of a sandbox enforcement check.
#[derive(Debug, Clone, PartialEq)]
pub enum SandboxCheckResult {
    Allowed,
    RateLimited { retry_after_secs: u64 },
    BudgetExceeded { spent_usd: f64, limit_usd: f64 },
    TokenLimitExceeded { used: u32, max: u32 },
    Timeout { elapsed_secs: u64 },
    NetworkDenied,
}

// ── Voice types ────────────────────────────────────────────────────────────────

/// Voice output format for TTS playback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VoiceOutputFormat {
    #[default]
    Wav,
    Mp3,
    Ogg,
    Raw,
}

/// Voice pipeline configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoicePipelineConfig {
    /// Maximum recording duration before auto-stop.
    pub max_duration_secs: u64,
    /// Audio sample rate in Hz (8000, 16000, 24000, 44100, 48000).
    pub sample_rate: u32,
    /// BCP-47 language tag for STT.
    pub language: String,
    /// Enable text-to-speech output.
    pub tts_enabled: bool,
    /// Enable speech-to-text input.
    pub stt_enabled: bool,
    /// Output audio format.
    pub output_format: VoiceOutputFormat,
}

impl Default for VoicePipelineConfig {
    fn default() -> Self {
        Self {
            max_duration_secs: 60,
            sample_rate: 16000,
            language: "en".to_string(),
            tts_enabled: true,
            stt_enabled: true,
            output_format: VoiceOutputFormat::Wav,
        }
    }
}

/// Playback state for a voice interaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VoicePlaybackStatus {
    Idle,
    Recording,
    Processing,
    Playing,
    #[default]
    Complete,
}

/// A single turn in a voice conversation (one input → one output).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInteraction {
    pub id: Uuid,
    /// STT-transcribed text that initiated this turn.
    pub stt_input: Option<String>,
    /// TTS-delivered response text.
    pub tts_output: Option<String>,
    /// Whether TTS is currently playing (applied after receiving the response).
    pub playback_status: VoicePlaybackStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// State of the voice pipeline FSM.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum VoicePipelineState {
    #[default]
    Idle,
    Recording,
    SttComplete,
    Processing,
    TtsComplete,
}

#[cfg(test)]
mod model_tests {
    use super::*;

    #[test]
    fn test_prompt_default() {
        let p = Prompt::default();
        assert_eq!(p.name, "untitled");
        assert_eq!(p.status, Status::Draft);
    }

    #[test]
    fn test_agent_identity_default() {
        let a = AgentIdentity::default();
        assert_eq!(a.name, "anonymous");
        assert!(a.capabilities.is_empty());
    }

    #[test]
    fn test_agent_identity_local_operator_has_owner_capabilities() {
        let a = AgentIdentity::local_operator("local-operator");
        assert_eq!(a.name, "local-operator");
        // Read + Write + Admin together cover every Action (read, write/lock/
        // evolve, delete/admin/transfer) so the local CLI works out of the box.
        for cap in [Capability::Read, Capability::Write, Capability::Admin] {
            assert!(a.capabilities.contains(&cap), "missing {cap:?}");
        }
    }

    #[test]
    fn test_capability_equality() {
        assert_eq!(Capability::Read, Capability::Read);
        assert_ne!(Capability::Read, Capability::Write);
    }

    #[test]
    fn test_search_mode_variants() {
        let modes = [SearchMode::Fast, SearchMode::Smart, SearchMode::Hybrid];
        assert_eq!(modes.len(), 3);
    }

    #[test]
    fn test_pagination_default() {
        let p = Pagination::default();
        assert_eq!(p.page, 1);
        assert!(p.per_page > 0);
    }

    #[test]
    fn test_paginated_generic() {
        let p: Paginated<String> = Paginated {
            items: vec!["a".to_string(), "b".to_string()],
            total: 2,
            page: 1,
            per_page: 10,
        };
        assert_eq!(p.total, 2);
    }

    #[test]
    fn test_scored_prompt() {
        let sp = ScoredPrompt {
            prompt: Prompt::default(),
            score: 0.95,
            matched_field: Some("name".to_string()),
        };
        assert!(sp.score > 0.0);
    }

    #[test]
    fn test_conflict_variants() {
        let c = Conflict::MissingRole;
        assert!(matches!(c, Conflict::MissingRole));
    }

    #[test]
    fn test_privacy_report_default() {
        let pr = PrivacyReport {
            issues: vec![],
            sanitized: false,
            secrets_found: 0,
            pii_found: 0,
            risk_level: "low".to_string(),
        };
        assert_eq!(pr.risk_level, "low");
    }

    #[test]
    fn test_prompt_version_creation() {
        let pv = PromptVersion {
            id: Uuid::new_v4(),
            prompt_id: Uuid::new_v4(),
            parent_id: None,
            version: semver::Version::new(1, 0, 0),
            changelog: "Initial".to_string(),
            diff: "+".to_string(),
            created_at: Utc::now(),
        };
        assert_eq!(pv.version.major, 1);
    }

    #[test]
    fn test_budget_manager() {
        let bm = BudgetManager {
            user_id: Uuid::new_v4(),
            monthly_budget_usd: 100.0,
            current_month_spend: 50.0,
            alert_threshold: 0.8,
            hard_limit: true,
            reset_date: Utc::now(),
        };
        assert!(bm.hard_limit);
    }

    #[test]
    fn test_llm_provider() {
        let lp = LLMProvider {
            name: "openai".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
            base_url: "https://api.openai.com".to_string(),
            model: "gpt-4".to_string(),
            priority: 1,
            cost_per_1k_tokens: 0.03,
            max_tokens: 8192,
            timeout_seconds: 30,
        };
        assert_eq!(lp.model, "gpt-4");
    }

    #[test]
    fn test_canary_deployment() {
        let cd = CanaryDeployment {
            feature: "new-search".to_string(),
            canary_percentage: 5.0,
            target_users: vec![],
            rollback_threshold: 0.05,
        };
        assert_eq!(cd.canary_percentage, 5.0);
    }

    #[test]
    fn test_user_correction() {
        let uc = UserCorrection {
            original_intent: "build app".to_string(),
            corrected_output: "use react".to_string(),
            feedback: "prefer a component framework".to_string(),
            agent_id: Uuid::new_v4(),
            timestamp: Utc::now(),
        };
        assert!(!uc.corrected_output.is_empty());
        assert!(!uc.feedback.is_empty());
    }

    #[test]
    fn test_prompt_patch_default() {
        let patch = PromptPatch::default();
        assert!(patch.name.is_none());
    }

    #[test]
    fn test_graduated_rollout_config() {
        let cfg = GraduatedRolloutConfig {
            rollout_id: "test".into(),
            feature: "new-search".into(),
            segments: vec![],
            auto_rollback: AutoRollbackPolicy::OnErrorRate { threshold: 0.05 },
            active: true,
        };
        assert!(cfg.active);
    }

    // ── Sandbox model tests (cycle-65) ───────────────────────────────

    #[test]
    fn test_sandbox_mode_default() {
        let mode = SandboxMode::default();
        assert!(matches!(mode, SandboxMode::Unrestricted));
    }

    #[test]
    fn test_sandbox_config_default() {
        let cfg = SandboxConfig::default();
        assert_eq!(cfg.max_tokens, 8192);
        assert!((cfg.max_cost_usd - 10.0).abs() < f64::EPSILON);
        assert_eq!(cfg.timeout_secs, 60);
        assert_eq!(cfg.max_memory_bytes, 268_435_456);
        assert_eq!(cfg.rate_limit_per_min, 60);
        assert!(cfg.deny_network);
    }

    #[test]
    fn test_sandbox_check_result_variants() {
        let allowed = SandboxCheckResult::Allowed;
        assert!(matches!(allowed, SandboxCheckResult::Allowed));

        let rate_limited = SandboxCheckResult::RateLimited {
            retry_after_secs: 30,
        };
        assert!(matches!(
            rate_limited,
            SandboxCheckResult::RateLimited { .. }
        ));

        let budget = SandboxCheckResult::BudgetExceeded {
            spent_usd: 12.5,
            limit_usd: 10.0,
        };
        assert!(matches!(budget, SandboxCheckResult::BudgetExceeded { .. }));

        let token = SandboxCheckResult::TokenLimitExceeded {
            used: 9000,
            max: 8192,
        };
        assert!(matches!(
            token,
            SandboxCheckResult::TokenLimitExceeded { .. }
        ));

        let timeout = SandboxCheckResult::Timeout { elapsed_secs: 61 };
        assert!(matches!(timeout, SandboxCheckResult::Timeout { .. }));

        let network = SandboxCheckResult::NetworkDenied;
        assert!(matches!(network, SandboxCheckResult::NetworkDenied));
    }
}
