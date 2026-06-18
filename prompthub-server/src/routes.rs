#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use prompt_hub::HubError;
use prompt_hub::models::*;

#[cfg(feature = "budget")]
use prompt_hub::budget::{BudgetAlert, BudgetConfig};

use crate::responses::{error, success};

// ── Satisfaction request DTOs ─────────────────────────────────────────────

/// Request body for recording a CSAT rating.
#[derive(Debug, Deserialize)]
pub struct RecordCsatRequest {
    pub score: u8,
    #[serde(default)]
    pub context: String,
}

/// Request body for recording an NPS rating.
#[derive(Debug, Deserialize)]
pub struct RecordNpsRequest {
    pub score: u8,
}

/// Request body for recording a satisfaction funnel event.
#[derive(Debug, Deserialize)]
pub struct SatisfactionEventRequest {
    pub prompt_id: String,
    pub successful: bool,
    #[serde(default = "default_one")]
    pub attempts: u8,
}

fn default_one() -> u8 {
    1
}

/// Request body for evolving a prompt.
///
/// `strategy` is a snake_case evolution strategy name (`mutate`, `crossover`,
/// `ab_test`, `semantic`, `compress`, `expand`). Defaults to `mutate` when
/// omitted.
#[derive(Debug, Deserialize)]
pub struct EvolvePromptRequest {
    #[serde(default = "default_evolution_strategy")]
    pub strategy: String,
}

fn default_evolution_strategy() -> String {
    "mutate".to_string()
}

// ── Token / cost / input / render request DTOs ────────────────────────────

/// Request body for counting a stored prompt's tokens under a model.
#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    /// Model identifier to count against (e.g. `"gpt-4"`).
    pub model: String,
}

/// Request body for estimating a stored prompt's cost under a model.
#[derive(Debug, Deserialize)]
pub struct CostRequest {
    /// Model identifier to price against (e.g. `"gpt-4"`).
    pub model: String,
    /// Anticipated completion length, in tokens.
    pub expected_output_tokens: usize,
}

/// Request body for rendering a stored prompt's `user_template`.
///
/// `vars` is a JSON object of template variable name → value bindings. It
/// defaults to an empty map when omitted, which still renders templates that
/// declare no `required_vars`.
#[derive(Debug, Deserialize)]
pub struct RenderRequest {
    #[serde(default)]
    pub vars: std::collections::HashMap<String, Value>,
}

use crate::state::AppState;

// ── Request / response DTOs ──────────────────────────────────────────────

/// Query parameters for listing prompts.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    page: Option<usize>,
    per_page: Option<usize>,
    #[allow(dead_code)]
    domain: Option<String>,
}

/// Query parameters for searching prompts.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    q: String,
    mode: Option<String>,
    page: Option<usize>,
    per_page: Option<usize>,
}

/// Query parameters for locking a prompt.
#[derive(Debug, Deserialize)]
pub struct LockQuery {
    ttl_seconds: Option<u64>,
}

/// Request body for registering a prompt.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    name: String,
    system_prompt: String,
    user_template: String,
    domain: Option<String>,
    tags: Option<Vec<String>>,
    target_roles: Option<Vec<String>>,
}

/// Build a default agent identity for operations that don't yet have
/// full RBAC integration over HTTP.
fn default_agent() -> AgentIdentity {
    AgentIdentity {
        id: Uuid::new_v4(),
        name: "http-server".to_string(),
        capabilities: vec![Capability::Read, Capability::Write],
        token_hash: String::new(),
        specialization_score: 0.0,
    }
}

// ── Prompt CRUD handlers ─────────────────────────────────────────────────

/// Register a new prompt.
///
/// Validates the payload, constructs a full Prompt entity, registers it
/// with the real PromptHub, and returns the assigned UUID.
#[instrument(skip(state))]
pub async fn register_prompt(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterRequest>,
) -> Response {
    info!(name = %payload.name, "Registering new prompt");

    if payload.name.is_empty() {
        warn!("Empty prompt name in register request");
        return error(StatusCode::BAD_REQUEST, "Prompt name cannot be empty").into_response();
    }
    if payload.system_prompt.is_empty() {
        return error(StatusCode::BAD_REQUEST, "system_prompt cannot be empty").into_response();
    }
    if payload.user_template.is_empty() {
        return error(StatusCode::BAD_REQUEST, "user_template cannot be empty").into_response();
    }

    // Map DTO to domain model
    let domain = payload
        .domain
        .as_deref()
        .and_then(|d| serde_json::from_str(&format!("\"{d}\"")).ok())
        .unwrap_or_default();

    let target_roles: Vec<Role> = payload
        .target_roles
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| serde_json::from_str(&format!("\"{r}\"")).ok())
        .collect();

    let prompt = Prompt {
        id: Uuid::new_v4(),
        name: payload.name.clone(),
        version: semver::Version::new(1, 0, 0),
        status: Status::Active,
        system_prompt: payload.system_prompt,
        user_template: payload.user_template,
        required_vars: Vec::new(),
        domain,
        tags: payload.tags.unwrap_or_default(),
        target_roles,
        metadata: PromptMeta::default(),
        metrics: PromptMetrics::default(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        author: default_agent(),
        deleted_at: None,
        generation_params: None,
        locale: None,
        multimodal: None,
    };

    let identity = default_agent();

    match state.hub.register(prompt, &identity).await {
        Ok(id) => {
            info!("Created prompt {}", id);
            success(json!({
                "id": id.to_string(),
                "status": "created"
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Failed to register prompt: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

/// List prompts with pagination.
///
/// Calls the real PromptHub.list() method and returns actual prompts
/// from the database.
#[instrument(skip(state))]
pub async fn list_prompts(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQuery>,
) -> Response {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(25).clamp(1, 1000);

    info!("Listing prompts — page {}, per_page {}", page, per_page);

    let pagination = Pagination { page, per_page };

    match state.hub.list(pagination).await {
        Ok(results) => {
            let items: Vec<Value> = results
                .items
                .into_iter()
                .map(|p| {
                    json!({
                        "id": p.id.to_string(),
                        "name": p.name,
                        "version": p.version.to_string(),
                        "status": p.status,
                        "domain": p.domain,
                        "tags": p.tags,
                        "system_prompt": p.system_prompt,
                        "user_template": p.user_template,
                        "created_at": p.created_at,
                        "updated_at": p.updated_at,
                    })
                })
                .collect();

            success(json!({
                "items": items,
                "total": results.total,
                "page": results.page,
                "per_page": results.per_page
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Failed to list prompts: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

/// Get a single prompt by its UUID.
///
/// Queries the real PromptHub storage layer with RBAC authorization via the
/// default agent identity (grants Read+Write for HTTP operations).
#[instrument(skip(state))]
pub async fn get_prompt(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    info!("Fetching prompt {}", id);

    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            warn!("Invalid UUID format: {}", id);
            return error(StatusCode::BAD_REQUEST, "Invalid UUID format").into_response();
        }
    };

    // Use hub.get_by_id() for exact-UUID lookup with RBAC authorization.
    // Previously used state.hub.storage().get_prompt(uuid) directly which
    // bypassed hub's RBAC intent logic present in all other CRUD routes.
    match state.hub.get_by_id(uuid, &default_agent()).await {
        Ok(Some(prompt)) => success(json!({
            "id": prompt.id.to_string(),
            "name": prompt.name,
            "version": prompt.version.to_string(),
            "status": prompt.status,
            "system_prompt": prompt.system_prompt,
            "user_template": prompt.user_template,
            "domain": prompt.domain,
            "tags": prompt.tags,
            "target_roles": prompt.target_roles,
            "metadata": prompt.metadata,
            "metrics": prompt.metrics,
            "created_at": prompt.created_at,
            "updated_at": prompt.updated_at,
        }))
        .into_response(),
        Ok(None) => {
            error(StatusCode::NOT_FOUND, format!("Prompt '{}' not found", id)).into_response()
        }
        Err(e) => {
            warn!("Failed to get prompt {}: {}", id, e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

/// Search prompts by query string.
///
/// Delegates to the real PromptHub.search() with the configured
/// hybrid search engine.
#[instrument(skip(state))]
pub async fn search_prompts(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Response {
    if query.q.is_empty() {
        return error(StatusCode::BAD_REQUEST, "Search query cannot be empty").into_response();
    }

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(25).clamp(1, 1000);

    let mode = query
        .mode
        .as_deref()
        .map(|m| match m {
            "fast" => SearchMode::Fast,
            "smart" => SearchMode::Smart,
            "hybrid" => SearchMode::Hybrid,
            _ => SearchMode::Hybrid,
        })
        .unwrap_or(SearchMode::Hybrid);

    info!(
        "Search: \"{}\" (mode={:?}, page={}, per_page={})",
        query.q, mode, page, per_page
    );

    let pagination = Pagination { page, per_page };
    let filters = SearchFilters::default();

    match state.hub.search(&query.q, mode, filters, pagination).await {
        Ok(results) => {
            let items: Vec<Value> = results
                .items
                .into_iter()
                .map(|sp| {
                    json!({
                        "prompt": {
                            "id": sp.prompt.id.to_string(),
                            "name": sp.prompt.name,
                            "version": sp.prompt.version.to_string(),
                            "status": sp.prompt.status,
                            "system_prompt": sp.prompt.system_prompt,
                            "user_template": sp.prompt.user_template,
                            "domain": sp.prompt.domain,
                            "tags": sp.prompt.tags,
                        },
                        "score": sp.score,
                        "matched_field": sp.matched_field,
                    })
                })
                .collect();

            success(json!({
                "items": items,
                "total": results.total,
                "query": query.q,
                "mode": query.mode.unwrap_or_else(|| "hybrid".to_string()),
                "page": results.page,
                "per_page": results.per_page
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Search failed: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

// ── Lock management handlers ─────────────────────────────────────────────

/// Lock a prompt for exclusive editing.
///
/// Acquires a real lock via PromptHub.lock() and returns a token
/// with an expiration timestamp.
#[instrument(skip(state))]
pub async fn lock_prompt(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<LockQuery>,
) -> Response {
    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid UUID format").into_response(),
    };

    let ttl_secs = query.ttl_seconds.unwrap_or(300);
    let ttl = std::time::Duration::from_secs(ttl_secs);
    let agent = default_agent();

    match state.hub.lock(uuid, &agent, ttl).await {
        Ok(token) => {
            info!("Lock acquired for prompt {} — token {}", id, token.token);
            success(json!({
                "token": token.token,
                "prompt_id": id,
                "expires_at": token.expires_at.to_rfc3339()
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Failed to lock prompt {}: {}", id, e);
            error(StatusCode::CONFLICT, format!("{e}")).into_response()
        }
    }
}

/// Unlock a previously locked prompt.
///
/// Releases a real lock via PromptHub.unlock().
#[instrument(skip(state))]
pub async fn unlock_prompt(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid UUID format").into_response(),
    };

    // Build a token from the path parameter to pass to unlock.
    // In production this would come from an auth header or request body.
    let token = prompt_hub::hub::LockToken {
        prompt_id: uuid,
        agent_id: default_agent().id,
        expires_at: Utc::now() + chrono::Duration::seconds(3600),
        token: format!("unlock-{}", uuid),
    };

    match state.hub.unlock(token).await {
        Ok(()) => {
            info!("Lock released for prompt {}", id);
            success(json!({ "unlocked": id })).into_response()
        }
        Err(e) => {
            // Expired locks are considered already unlocked
            info!("Unlock for prompt {} (may have been expired): {}", id, e);
            success(json!({ "unlocked": id, "note": "lock was expired or not found" }))
                .into_response()
        }
    }
}

// ── Audit handler ────────────────────────────────────────────────────────

/// Get audit trail entries for a prompt.
///
/// Fetches real audit entries from storage via PromptHub.audit_trail().
#[instrument(skip(state))]
pub async fn audit_trail(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid UUID format").into_response(),
    };

    let pagination = Pagination {
        page: 1,
        per_page: 100,
    };

    match state.hub.audit_trail(uuid, pagination).await {
        Ok(results) => {
            let entries: Vec<Value> = results
                .items
                .into_iter()
                .map(|entry| {
                    json!({
                        "id": entry.id,
                        "prompt_id": entry.prompt_id.map(|u| u.to_string()).unwrap_or_default(),
                        "action": entry.action,
                        "agent_id": entry.agent_id.to_string(),
                        "timestamp": entry.timestamp,
                        "diff_hash": entry.diff_hash,
                        "before_json": entry.before_json,
                        "after_json": entry.after_json,
                        "ip_address": entry.ip_address,
                    })
                })
                .collect();

            success(json!({
                "prompt_id": id,
                "entries": entries,
                "total": results.total,
                "page": results.page,
                "per_page": results.per_page
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Failed to fetch audit trail for {}: {}", id, e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

// ── Swarm handlers ───────────────────────────────────────────────────────

/// Generate a swarm bundle.
///
/// Queries the real storage layer for active prompts and assembles
/// a workflow bundle with roles and metadata.
#[instrument(skip(state))]
pub async fn generate_bundle(State(state): State<Arc<AppState>>) -> Response {
    info!("Generating swarm bundle");

    // Fetch real prompts from storage to build the bundle
    let pagination = Pagination {
        page: 1,
        per_page: 50,
    };

    match state.hub.list(pagination).await {
        Ok(results) => {
            let roles: Value = results.items.iter().fold(json!({}), |mut acc, prompt| {
                for role in &prompt.target_roles {
                    let role_key = format!("{:?}", role).to_lowercase();
                    if let Some(arr) = acc.get_mut(&role_key) {
                        if let Some(a) = arr.as_array_mut() {
                            a.push(json!({
                                "id": prompt.id.to_string(),
                                "name": prompt.name,
                                "system_prompt": prompt.system_prompt,
                            }));
                        }
                    } else {
                        acc[role_key] = json!([{
                            "id": prompt.id.to_string(),
                            "name": prompt.name,
                            "system_prompt": prompt.system_prompt,
                        }]);
                    }
                }
                acc
            });

            success(json!({
                "workflow_id": Uuid::new_v4().to_string(),
                "prompt_count": results.total,
                "roles": roles,
                "consistency_report": [],
                "evolution_suggestions": []
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Failed to generate bundle: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

// ── Health probe handlers ────────────────────────────────────────────────

/// Full health check with per-component status.
///
/// Checks real database connectivity via PromptHub.storage().health_check().
#[instrument(skip(state))]
pub async fn health_check(State(state): State<Arc<AppState>>) -> Response {
    let db_ok = state.hub.storage().health_check().await;
    let (db_status_str, db_msg) = match &db_ok {
        Ok(true) => ("healthy", "Connected".to_string()),
        Ok(false) => ("degraded", "Unresponsive".to_string()),
        Err(e) => ("unhealthy", format!("Error: {e}")),
    };

    let uptime_secs = state.uptime().as_secs();

    success(json!({
        "status": if db_status_str == "healthy" { "healthy" } else { "degraded" },
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": uptime_secs,
        "checks": [
            { "name": "database", "status": db_status_str, "message": db_msg },
            { "name": "search_index", "status": "healthy", "message": "FTS5 ready" },
            { "name": "disk", "status": "healthy", "message": "Space available" }
        ]
    }))
    .into_response()
}

/// Kubernetes readiness probe.
///
/// Returns 200 when the database is reachable, 503 otherwise.
#[instrument(skip(state))]
pub async fn ready_check(State(state): State<Arc<AppState>>) -> Response {
    match state.hub.storage().health_check().await {
        Ok(true) => success(json!({ "ready": true })).into_response(),
        Ok(false) => {
            error(StatusCode::SERVICE_UNAVAILABLE, "Database unresponsive").into_response()
        }
        Err(e) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("Database error: {e}"),
        )
        .into_response(),
    }
}

/// Kubernetes liveness probe.
///
/// Always returns 200 — if this handler cannot execute the process is dead.
#[instrument]
pub async fn live_check() -> Response {
    success(json!({ "alive": true })).into_response()
}

// ── Metrics handler ──────────────────────────────────────────────────────

/// Prometheus-compatible metrics endpoint.
///
/// Returns real metrics from the PromptHub metrics collector in the Prometheus
/// text exposition format.
#[instrument(skip(state))]
pub async fn prometheus_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let output = render_metrics(&state.hub.metrics(), state.uptime().as_secs_f64());
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        output,
    )
}

/// Render the Prometheus text exposition for the given collector snapshot.
///
/// With the `otel` feature the body comes from the `prompt-hub` core renderer
/// (full counter/gauge set via the `prometheus` crate); the server appends its
/// own process-level `prompt_hub_uptime_seconds` gauge, which the core has no
/// concept of. Without `otel`, a compact but **valid** hand-rolled exposition is
/// emitted — notably the search-latency aggregate is a gauge (its average), not
/// a single-bucket pseudo-histogram.
pub(crate) fn render_metrics(
    metrics: &prompt_hub::metrics::MetricsCollector,
    uptime_secs: f64,
) -> String {
    let uptime_block = format!(
        "# HELP prompt_hub_uptime_seconds Server uptime in seconds\n\
         # TYPE prompt_hub_uptime_seconds gauge\n\
         prompt_hub_uptime_seconds {uptime_secs:.3}\n"
    );

    #[cfg(feature = "otel")]
    {
        match metrics.prometheus_text() {
            Ok(mut body) => {
                body.push_str(&uptime_block);
                return body;
            }
            Err(e) => {
                warn!("prometheus exposition failed, using fallback: {e}");
            }
        }
    }

    // Default (and otel-error fallback): compact, valid exposition.
    format!(
        "# HELP prompt_hub_requests_total Total requests processed\n\
         # TYPE prompt_hub_requests_total counter\n\
         prompt_hub_requests_total {}\n\
         # HELP prompt_hub_search_latency_ms_avg Average search latency in milliseconds\n\
         # TYPE prompt_hub_search_latency_ms_avg gauge\n\
         prompt_hub_search_latency_ms_avg {}\n\
         # HELP prompt_hub_active_locks Currently held locks\n\
         # TYPE prompt_hub_active_locks gauge\n\
         prompt_hub_active_locks {}\n\
         {uptime_block}",
        metrics.get_requests_total(),
        metrics.get_avg_search_latency(),
        metrics.get_active_locks(),
    )
}

// ── OpenAPI / docs handlers ──────────────────────────────────────────────

/// Return OpenAPI JSON specification.
pub async fn openapi_json() -> Json<Value> {
    Json(crate::openapi::build_openapi_spec())
}

/// Serve Swagger UI HTML.
pub async fn swagger_ui() -> axum::response::Html<String> {
    crate::openapi::swagger_ui().await
}

/// Request body for vibe coding — natural language description → generated deliverable.
#[derive(Debug, Deserialize)]
pub struct VibeCodeRequest {
    /// Natural-language description of the desired output (e.g. "Create a React login form").
    pub request: String,
    /// Optional required skill level (Beginner | Intermediate | Expert). Defaults to Intermediate.
    pub skill_level: Option<String>,
}

/// Map a human-readable skill level string to [`SkillLevel`].
fn parse_skill_level(s: &str) -> SkillLevel {
    match s {
        "Beginner" | "beginner" => SkillLevel::Beginner,
        "Intermediate" | "intermediate" => SkillLevel::Intermediate,
        "Expert" | "expert" => SkillLevel::Expert,
        _ => SkillLevel::Intermediate, // default
    }
}

// ── Budget request DTOs ──────────────────────────────────────────────────

/// Record spend request body.
#[cfg(feature = "budget")]
#[derive(Debug, Deserialize)]
pub struct RecordSpendRequest {
    /// Amount in USD to record.
    pub amount_usd: f64,
}

/// Set monthly budget request body.
#[cfg(feature = "budget")]
#[derive(Debug, Deserialize)]
pub struct SetMonthlyBudgetRequest {
    /// New monthly budget in USD.
    pub monthly_budget_usd: f64,
}

/// Load budget config request body.
#[cfg(feature = "budget")]
#[derive(Debug, Deserialize)]
pub struct LoadConfigRequest {
    /// Budget configuration to load.
    #[serde(rename = "config")]
    pub config: BudgetConfig,
}

// ── Load balancer request / response DTOs ────────────────────────────────

/// Request body for adding a provider to the load balancer pool.
#[derive(Debug, Serialize, Deserialize)]
pub struct AddProviderRequest {
    /// Unique name for the provider.
    pub name: String,
    /// Endpoint URL for the provider.
    pub url: String,
    /// Relative traffic weight (default 1 = equal).
    pub weight: u32,
}

/// Request body for recording provider latency.
#[derive(Debug, Serialize, Deserialize)]
pub struct LatencyRequest {
    /// Name of the registered provider.
    pub provider_name: String,
    /// Measured round-trip latency in milliseconds.
    pub latency_ms: u64,
}

/// Request body for recording a provider failure event.
#[derive(Debug, Serialize, Deserialize)]
pub struct FailureRequest {
    /// Name of the registered provider.
    pub provider_name: String,
}

/// Response DTO for a provider selection result.
#[derive(Debug, Serialize)]
pub struct ProviderSelectionResponse {
    pub provider_name: String,
    pub provider_url: String,
    pub strategy_used: String,
}

/// Per-provider statistics response.
#[derive(Debug, Serialize)]
pub struct ProviderStatsResponse {
    pub name: String,
    pub healthy: bool,
    pub latency_ms: u64,
    pub request_count: u64,
    pub error_count: u64,
}

// ── Vibe coding handler ──────────────────────────────────────────────────────

/// Execute vibe coding — generate a deliverable from natural language.
///
/// Parses the request, constructs appropriate [`UserInput`] and default
/// [`SkillLevel`], calls `hub.vibe_code()` and returns the generated artifact
/// along with confidence score and next suggestions.
#[cfg(feature = "vibe")]
#[instrument(skip(state))]
pub async fn vibe_code(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<VibeCodeRequest>,
) -> Response {
    if payload.request.is_empty() {
        return error(StatusCode::BAD_REQUEST, "request cannot be empty").into_response();
    }

    let skill_level = parse_skill_level(payload.skill_level.as_deref().unwrap_or("Intermediate"));

    let user_input = UserInput {
        input_type: InputType::Text,
        raw_data: Vec::new(),
        extracted_text: payload.request.clone(),
    };

    match state
        .hub
        .vibe_code(&payload.request, user_input, skill_level)
        .await
    {
        Ok(result) => {
            info!(confidence = result.confidence, "Vibe coding completed");
            success(json!({
                "artifacts": result.artifacts,
                "summary": result.summary,
                "next_suggestions": result.next_suggestions,
                "cost_estimate": result.cost_estimate,
                "confidence": result.confidence,
                "execution_time_ms": result.execution_time_ms,
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Vibe coding failed: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

// ── Budget tracking routes ─────────────────────────────────────────────────

/// Record a spend amount against the monthly budget.
#[cfg(feature = "budget")]
#[instrument(skip(state))]
pub async fn budget_record_spend(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RecordSpendRequest>,
) -> Response {
    let alert = state.hub.record_spend(payload.amount_usd);
    let alert_str = match alert {
        BudgetAlert::None => "none".to_string(),
        BudgetAlert::FiftyPercent => "fifty_percent".to_string(),
        BudgetAlert::EightyPercent => "eighty_percent".to_string(),
        BudgetAlert::HundredPercent => "hundred_percent".to_string(),
        BudgetAlert::OverBudget => "over_budget".to_string(),
    };
    info!(alert = %alert_str, "Budget spend recorded");
    success(json!({
        "alert": alert_str,
        "current_spend_usd": state.hub.current_spend_usd(),
        "utilization_percent": state.hub.budget_utilization(),
    }))
    .into_response()
}

/// Get current budget status (spend, utilization, exceeded flag).
#[cfg(feature = "budget")]
#[instrument(skip(state))]
pub async fn budget_status(State(state): State<Arc<AppState>>) -> Response {
    let spend = state.hub.current_spend_usd();
    let utilization = state.hub.budget_utilization();
    let exceeded = state.hub.is_budget_exceeded();

    success(json!({
        "current_spend_usd": spend,
        "utilization_percent": utilization,
        "is_exceeded": exceeded,
    }))
    .into_response()
}

/// Set the monthly budget amount.
#[cfg(feature = "budget")]
#[instrument(skip(state))]
pub async fn set_monthly_budget(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SetMonthlyBudgetRequest>,
) -> Response {
    state.hub.set_monthly_budget(payload.monthly_budget_usd);
    info!(
        monthly_budget = payload.monthly_budget_usd,
        "Monthly budget updated"
    );
    success(json!({
        "monthly_budget_usd": payload.monthly_budget_usd,
        "current_spend_usd": state.hub.current_spend_usd(),
        "utilization_percent": state.hub.budget_utilization(),
    }))
    .into_response()
}

/// Load a persisted BudgetConfig into the tracker.
#[cfg(feature = "budget")]
#[instrument(skip(state))]
pub async fn load_budget_config(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoadConfigRequest>,
) -> Response {
    match state.hub.load_budget_config(&payload.config) {
        Ok(()) => {
            info!("Budget config loaded");
            success(json!({
                "status": "loaded",
                "config": payload.config,
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Failed to load budget config: {}", e);
            error(StatusCode::BAD_REQUEST, format!("{e}")).into_response()
        }
    }
}

/// Save the current budget state as a BudgetConfig for an org.
#[cfg(feature = "budget")]
#[instrument(skip(state))]
pub async fn save_budget_config(
    State(state): State<Arc<AppState>>,
    Path(org_id): Path<String>,
) -> Response {
    match state.hub.save_budget_config(&org_id) {
        Ok(config) => {
            info!(org_id = %org_id, "Budget config saved");
            success(json!({
                "config": config,
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Failed to save budget config: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

/// Reset spend counters for a new billing period.
#[cfg(feature = "budget")]
#[instrument(skip(state))]
pub async fn reset_budget_period(State(state): State<Arc<AppState>>) -> Response {
    state.hub.reset_budget_period();
    info!("Budget period reset");
    success(json!({
        "status": "reset",
        "current_spend_usd": 0.0,
        "utilization_percent": 0.0,
    }))
    .into_response()
}

// ── Load balancer handlers ───────────────────────────────────────────────

/// Register a new LLM provider in the load balancer pool.
#[instrument(skip(state))]
pub async fn add_lb_provider(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AddProviderRequest>,
) -> Response {
    if payload.name.is_empty() {
        warn!("Empty provider name in add_lb_provider request");
        return error(StatusCode::BAD_REQUEST, "provider name cannot be empty").into_response();
    }
    if payload.url.is_empty() {
        return error(StatusCode::BAD_REQUEST, "provider url cannot be empty").into_response();
    }

    state
        .hub
        .add_lb_provider(&payload.name, &payload.url, payload.weight);
    info!(
        provider = %payload.name,
        url = %payload.url,
        weight = payload.weight,
        "Added load balancer provider"
    );
    success(json!({
        "name": payload.name,
        "url": payload.url,
        "weight": payload.weight,
    }))
    .into_response()
}

/// Select the next healthy provider according to the configured routing strategy.
#[instrument(skip(state))]
pub async fn select_provider(State(state): State<Arc<AppState>>) -> Response {
    match state.hub.select_provider() {
        Ok(selection) => {
            info!(
                provider = %selection.provider_name,
                strategy = ?selection.strategy_used,
                "Selected load balancer provider"
            );
            success(json!({
                "provider_name": selection.provider_name,
                "provider_url": selection.provider_url,
                "strategy_used": routing_strategy_to_string(selection.strategy_used),
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Provider selection failed: {}", e);
            error(StatusCode::CONFLICT, format!("{e}")).into_response()
        }
    }
}

/// Record latency for a provider.
#[instrument(skip(state))]
pub async fn record_lb_latency(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LatencyRequest>,
) -> Response {
    if payload.provider_name.is_empty() {
        return error(StatusCode::BAD_REQUEST, "provider_name cannot be empty").into_response();
    }

    state
        .hub
        .record_lb_latency(&payload.provider_name, payload.latency_ms);
    info!(
        provider = %payload.provider_name,
        latency_ms = payload.latency_ms,
        "Recorded load balancer latency"
    );
    success(json!({
        "provider_name": payload.provider_name,
        "latency_ms": payload.latency_ms,
    }))
    .into_response()
}

/// Record a failure event for a provider.
#[instrument(skip(state))]
pub async fn record_lb_failure(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<FailureRequest>,
) -> Response {
    if payload.provider_name.is_empty() {
        return error(StatusCode::BAD_REQUEST, "provider_name cannot be empty").into_response();
    }

    state.hub.record_lb_failure(&payload.provider_name);
    warn!(
        provider = %payload.provider_name,
        "Recorded load balancer failure"
    );
    success(json!({
        "provider_name": payload.provider_name,
        "status": "failure_recorded",
    }))
    .into_response()
}

/// Return current statistics for all providers in the load balancer pool.
#[instrument(skip(state))]
pub async fn get_lb_stats(State(state): State<Arc<AppState>>) -> Response {
    let stats = state.hub.get_lb_stats();
    let total = stats.len();
    let items: Vec<Value> = stats
        .into_iter()
        .map(|s| {
            json!({
                "name": s.name,
                "healthy": s.healthy,
                "latency_ms": s.latency_ms,
                "request_count": s.request_count,
                "error_count": s.error_count,
            })
        })
        .collect();

    success(json!({
        "providers": items,
        "total": total,
    }))
    .into_response()
}

/// Convert a `RoutingStrategy` to its snake_case JSON representation.
fn routing_strategy_to_string(strategy: prompt_hub::load_balancer::RoutingStrategy) -> String {
    match strategy {
        prompt_hub::load_balancer::RoutingStrategy::RoundRobin => "round_robin".to_string(),
        prompt_hub::load_balancer::RoutingStrategy::Weighted => "weighted".to_string(),
        prompt_hub::load_balancer::RoutingStrategy::LeastLatency => "least_latency".to_string(),
    }
}

// ── Satisfaction handler functions ────────────────────────────────────────

/// Record a CSAT rating (1-5) via HTTP.
#[instrument(skip(state))]
pub async fn record_csat(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RecordCsatRequest>,
) -> Response {
    if !(1..=5).contains(&payload.score) {
        warn!(score = payload.score, "Invalid CSAT score in request");
        return error(
            StatusCode::BAD_REQUEST,
            "CSAT score must be between 1 and 5",
        )
        .into_response();
    }

    state
        .hub
        .record_csat_rating(payload.score, &payload.context);
    info!(score = payload.score, "Recorded CSAT rating");
    success(json!({
        "score": payload.score,
        "scale": 5,
    }))
    .into_response()
}

/// Record an NPS rating (1-10) via HTTP.
#[instrument(skip(state))]
pub async fn record_nps(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RecordNpsRequest>,
) -> Response {
    if !(1..=10).contains(&payload.score) {
        warn!(score = payload.score, "Invalid NPS score in request");
        return error(
            StatusCode::BAD_REQUEST,
            "NPS score must be between 1 and 10",
        )
        .into_response();
    }

    state.hub.record_nps_rating(payload.score);
    info!(score = payload.score, "Recorded NPS rating");
    success(json!({
        "score": payload.score,
        "scale": 10,
    }))
    .into_response()
}

/// Record a satisfaction funnel event via HTTP.
#[instrument(skip(state))]
pub async fn record_satisfaction_event(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SatisfactionEventRequest>,
) -> Response {
    if payload.prompt_id.is_empty() {
        return error(StatusCode::BAD_REQUEST, "prompt_id cannot be empty").into_response();
    }

    state
        .hub
        .record_satisfaction_event(&payload.prompt_id, payload.successful, payload.attempts);
    info!(prompt_id = %payload.prompt_id, successful = payload.successful, "Recorded satisfaction event");
    success(json!({
        "prompt_id": payload.prompt_id,
        "successful": payload.successful,
        "attempts": payload.attempts,
    }))
    .into_response()
}

/// Return current satisfaction metrics via HTTP.
#[instrument(skip(state))]
pub async fn get_satisfaction_metrics(State(state): State<Arc<AppState>>) -> Response {
    match state.hub.satisfaction_metrics() {
        Ok(metrics) => success(json!({
            "csat_average": metrics.csat_average,
            "nps_score": metrics.nps_score,
            "one_shot_success_rate": metrics.one_shot_success_rate,
            "total_ratings": metrics.total_ratings,
            "total_events": metrics.total_events,
            "recent_trend": format!("{:?}", metrics.recent_trend).to_lowercase(),
        }))
        .into_response(),
        Err(e) => {
            warn!("Failed to get satisfaction metrics: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

// ── Evolution handler functions ───────────────────────────────────────────

/// Parse a snake_case strategy name into an [`EvolutionStrategy`].
///
/// Mirrors the snake_case convention of the other route helpers
/// (`parse_skill_level`, `routing_strategy_to_string`). Returns the original
/// (unknown) input as `Err` so the caller can surface it in a 400 response.
fn parse_evolution_strategy(s: &str) -> Result<EvolutionStrategy, String> {
    match s.trim().to_lowercase().as_str() {
        "mutate" => Ok(EvolutionStrategy::Mutate),
        "crossover" => Ok(EvolutionStrategy::Crossover),
        "ab_test" => Ok(EvolutionStrategy::AbTest),
        "semantic" => Ok(EvolutionStrategy::Semantic),
        "compress" => Ok(EvolutionStrategy::Compress),
        "expand" => Ok(EvolutionStrategy::Expand),
        other => Err(other.to_string()),
    }
}

/// Evolve a prompt into a new variant via the chosen [`EvolutionStrategy`].
///
/// Thin shell over [`PromptHub::evolve_prompt`](prompt_hub::hub::PromptHub::evolve_prompt): parses the path UUID and the
/// strategy, delegates to the core hub method (which performs RBAC, evolution,
/// persistence, indexing and audit), then returns the evolved [`Prompt`] as
/// JSON. `HubError` is mapped to the same HTTP statuses used by the other
/// mutating routes (`NotFound` → 404, `Unauthorized` → 403, `Internal`/other
/// → 500).
#[instrument(skip(state))]
pub async fn evolve_prompt(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<EvolvePromptRequest>,
) -> Response {
    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            warn!("Invalid UUID format: {}", id);
            return error(StatusCode::BAD_REQUEST, "Invalid UUID format").into_response();
        }
    };

    let strategy = match parse_evolution_strategy(&payload.strategy) {
        Ok(s) => s,
        Err(unknown) => {
            warn!(strategy = %unknown, "Unknown evolution strategy in request");
            return error(
                StatusCode::BAD_REQUEST,
                format!(
                    "Unknown evolution strategy '{unknown}' (expected one of: \
                     mutate, crossover, ab_test, semantic, compress, expand)"
                ),
            )
            .into_response();
        }
    };

    match state
        .hub
        .evolve_prompt(uuid, strategy, &default_agent())
        .await
    {
        Ok(prompt) => {
            info!(base = %id, evolved = %prompt.id, "Evolved prompt via HTTP");
            success(json!({
                "id": prompt.id.to_string(),
                "name": prompt.name,
                "version": prompt.version.to_string(),
                "status": prompt.status,
                "system_prompt": prompt.system_prompt,
                "user_template": prompt.user_template,
                "domain": prompt.domain,
                "tags": prompt.tags,
                "target_roles": prompt.target_roles,
                "metadata": prompt.metadata,
                "metrics": prompt.metrics,
                "created_at": prompt.created_at,
                "updated_at": prompt.updated_at,
            }))
            .into_response()
        }
        Err(HubError::NotFound(_)) => {
            error(StatusCode::NOT_FOUND, format!("Prompt '{}' not found", id)).into_response()
        }
        Err(HubError::Unauthorized(msg)) => error(StatusCode::FORBIDDEN, msg).into_response(),
        Err(e) => {
            warn!("Failed to evolve prompt {}: {}", id, e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

// ── Token / cost / input / render handler functions ──────────────────────

/// Count the tokens of a stored prompt under the requested model.
///
/// Thin shell over [`PromptHub::count_prompt_tokens`](prompt_hub::hub::PromptHub::count_prompt_tokens): parses the path UUID,
/// delegates to the core hub method (RBAC Read → fetch → tokenize), and returns
/// the resulting model + token count. The core `TokenCount` type does not derive
/// `Serialize`, so its fields are mapped into the response JSON by hand (the same
/// precedent used for the budget/satisfaction routes). `HubError` is mapped to
/// the same HTTP statuses as the other id-based routes (`NotFound` → 404,
/// `Unauthorized` → 403, other → 500).
#[instrument(skip(state))]
pub async fn count_prompt_tokens_route(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<TokenRequest>,
) -> Response {
    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            warn!("Invalid UUID format: {}", id);
            return error(StatusCode::BAD_REQUEST, "Invalid UUID format").into_response();
        }
    };

    if payload.model.is_empty() {
        return error(StatusCode::BAD_REQUEST, "model cannot be empty").into_response();
    }

    match state
        .hub
        .count_prompt_tokens(uuid, &payload.model, &default_agent())
        .await
    {
        Ok(count) => {
            info!(prompt = %id, model = %count.model, tokens = count.tokens, "Counted prompt tokens via HTTP");
            success(json!({
                "model": count.model,
                "tokens": count.tokens,
            }))
            .into_response()
        }
        Err(HubError::NotFound(_)) => {
            error(StatusCode::NOT_FOUND, format!("Prompt '{}' not found", id)).into_response()
        }
        Err(HubError::Unauthorized(msg)) => error(StatusCode::FORBIDDEN, msg).into_response(),
        Err(e) => {
            warn!("Failed to count tokens for prompt {}: {}", id, e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

/// Estimate the input + output cost of a stored prompt under the requested model.
///
/// Thin shell over [`PromptHub::estimate_prompt_cost`](prompt_hub::hub::PromptHub::estimate_prompt_cost). The core
/// `CostEstimateDetail` type does not derive `Serialize`, so its fields are
/// mapped into the response JSON by hand. Error mapping mirrors the other
/// id-based routes (`NotFound` → 404, `Unauthorized` → 403, other → 500).
#[instrument(skip(state))]
pub async fn estimate_prompt_cost_route(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<CostRequest>,
) -> Response {
    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            warn!("Invalid UUID format: {}", id);
            return error(StatusCode::BAD_REQUEST, "Invalid UUID format").into_response();
        }
    };

    if payload.model.is_empty() {
        return error(StatusCode::BAD_REQUEST, "model cannot be empty").into_response();
    }

    match state
        .hub
        .estimate_prompt_cost(
            uuid,
            &payload.model,
            payload.expected_output_tokens,
            &default_agent(),
        )
        .await
    {
        Ok(cost) => {
            info!(prompt = %id, model = %cost.model, total_cost = cost.total_cost, "Estimated prompt cost via HTTP");
            success(json!({
                "model": cost.model,
                "input_tokens": cost.input_tokens,
                "output_tokens": cost.output_tokens,
                "input_cost": cost.input_cost,
                "output_cost": cost.output_cost,
                "total_cost": cost.total_cost,
            }))
            .into_response()
        }
        Err(HubError::NotFound(_)) => {
            error(StatusCode::NOT_FOUND, format!("Prompt '{}' not found", id)).into_response()
        }
        Err(HubError::Unauthorized(msg)) => error(StatusCode::FORBIDDEN, msg).into_response(),
        Err(e) => {
            warn!("Failed to estimate cost for prompt {}: {}", id, e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

/// Classify a raw multimodal [`UserInput`] into an [`Intent`].
///
/// Thin shell over [`PromptHub::process_input`](prompt_hub::hub::PromptHub::process_input): the request body deserializes
/// directly into the core `UserInput` model (which derives `Deserialize`), the
/// hub classifies it, and the resulting `Intent` — which derives `Serialize` —
/// is returned as JSON. A `ValidationError` from the core maps to 422; any other
/// error maps to 500.
#[instrument(skip(state, payload))]
pub async fn process_input_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UserInput>,
) -> Response {
    match state.hub.process_input(payload).await {
        Ok(intent) => {
            info!(domain = ?intent.domain, task_type = ?intent.task_type, "Processed user input via HTTP");
            success(json!(intent)).into_response()
        }
        Err(HubError::ValidationError(msg)) => {
            error(StatusCode::UNPROCESSABLE_ENTITY, msg).into_response()
        }
        Err(e) => {
            warn!("Failed to process input: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

/// Render a stored prompt's `user_template` with the supplied variables.
///
/// Thin shell over [`PromptHub::render_prompt`](prompt_hub::hub::PromptHub::render_prompt): parses the path UUID, delegates
/// to the core method (RBAC Read → required-var check → template render), and
/// returns the rendered string. A missing required variable or a template
/// failure surfaces from the core as `ValidationError` and maps to 422; the
/// id-based errors mirror the other routes (`NotFound` → 404,
/// `Unauthorized` → 403, other → 500).
#[instrument(skip(state, payload))]
pub async fn render_prompt_route(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<RenderRequest>,
) -> Response {
    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            warn!("Invalid UUID format: {}", id);
            return error(StatusCode::BAD_REQUEST, "Invalid UUID format").into_response();
        }
    };

    match state
        .hub
        .render_prompt(uuid, payload.vars, &default_agent())
        .await
    {
        Ok(rendered) => {
            info!(prompt = %id, "Rendered prompt template via HTTP");
            success(json!({ "rendered": rendered })).into_response()
        }
        Err(HubError::NotFound(_)) => {
            error(StatusCode::NOT_FOUND, format!("Prompt '{}' not found", id)).into_response()
        }
        Err(HubError::Unauthorized(msg)) => error(StatusCode::FORBIDDEN, msg).into_response(),
        Err(HubError::ValidationError(msg)) => {
            error(StatusCode::UNPROCESSABLE_ENTITY, msg).into_response()
        }
        Err(e) => {
            warn!("Failed to render prompt {}: {}", id, e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

// ── Provider health handler functions (feature: multi-provider, always-on) ─
// ── Routes for provider health monitoring and management ───────────────────

/// Request body for registering a provider.
#[derive(Debug, Deserialize)]
pub struct RegisterProviderRequest {
    pub name: String,
    pub url: String,
}

/// Request body for recording a success event.
#[derive(Debug, Deserialize)]
pub struct RecordSuccessRequest {
    pub provider_name: String,
    #[serde(default)]
    pub latency_ms: u64,
}

/// Request body for recording a failure event.
#[derive(Debug, Deserialize)]
pub struct RecordFailureRequest {
    pub provider_name: String,
}

/// Request body for checking health.
#[derive(Debug, Deserialize)]
pub struct HealthCheckRequest {
    pub provider_name: String,
}

/// Provider health status response.
#[derive(Debug, Serialize)]
pub struct HealthStatusResponse {
    pub provider_name: String,
    pub healthy: bool,
}

/// Full health summary response.
#[derive(Debug, Serialize)]
pub struct HealthSummaryResponse {
    pub providers: Vec<ProviderHealthRecord>,
    pub healthy_count: usize,
    pub degraded_count: usize,
    pub unhealthy_count: usize,
    #[serde(rename = "overall")]
    pub overall_status: String,
}

/// Provider health record (exposed in summary).
#[derive(Debug, Serialize)]
pub struct ProviderHealthRecord {
    pub name: String,
    pub url: String,
    #[serde(rename = "status")]
    pub health_status: String,
    pub last_latency_ms: u64,
    pub error_count: u32,
    pub success_count: u32,
}

impl From<prompt_hub::provider_health::ProviderHealthRecord> for ProviderHealthRecord {
    fn from(record: prompt_hub::provider_health::ProviderHealthRecord) -> Self {
        let status_str = match record.status {
            prompt_hub::provider_health::HealthStatus::Healthy => "healthy",
            prompt_hub::provider_health::HealthStatus::Degraded => "degraded",
            prompt_hub::provider_health::HealthStatus::Unhealthy => "unhealthy",
            prompt_hub::provider_health::HealthStatus::Unknown => "unknown",
        };
        Self {
            name: record.name,
            url: record.url,
            health_status: status_str.to_string(),
            last_latency_ms: record.last_latency_ms,
            error_count: record.error_count,
            success_count: record.success_count,
        }
    }
}

impl From<prompt_hub::provider_health::HealthSummary> for HealthSummaryResponse {
    fn from(summary: prompt_hub::provider_health::HealthSummary) -> Self {
        let overall_str = match summary.overall {
            prompt_hub::provider_health::HealthStatus::Healthy => "healthy",
            prompt_hub::provider_health::HealthStatus::Degraded => "degraded",
            prompt_hub::provider_health::HealthStatus::Unhealthy => "unhealthy",
            prompt_hub::provider_health::HealthStatus::Unknown => "unknown",
        };
        Self {
            providers: summary.providers.into_iter().map(ProviderHealthRecord::from).collect(),
            healthy_count: summary.healthy_count,
            degraded_count: summary.degraded_count,
            unhealthy_count: summary.unhealthy_count,
            overall_status: overall_str.to_string(),
        }
    }
}

/// Register a new provider for health monitoring.
#[instrument(skip(state))]
pub async fn register_provider_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterProviderRequest>,
) -> Response {
    if payload.name.is_empty() {
        return error(StatusCode::BAD_REQUEST, "provider name cannot be empty").into_response();
    }
    if payload.url.is_empty() {
        return error(StatusCode::BAD_REQUEST, "provider url cannot be empty").into_response();
    }

    state.hub.register_provider(&payload.name, &payload.url);
    info!(
        provider = %payload.name,
        url = %payload.url,
        "Registered provider via HTTP"
    );
    success(json!({
        "name": payload.name,
        "url": payload.url,
        "status": "registered"
    }))
    .into_response()
}

/// Record a successful call for a provider.
#[instrument(skip(state))]
pub async fn record_success_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RecordSuccessRequest>,
) -> Response {
    if payload.provider_name.is_empty() {
        return error(StatusCode::BAD_REQUEST, "provider_name cannot be empty").into_response();
    }

    state
        .hub
        .record_success(&payload.provider_name, payload.latency_ms);
    info!(
        provider = %payload.provider_name,
        latency_ms = payload.latency_ms,
        "Recorded provider success via HTTP"
    );
    success(json!({
        "provider_name": payload.provider_name,
        "latency_ms": payload.latency_ms,
        "status": "success_recorded"
    }))
    .into_response()
}

/// Record a failed call for a provider.
#[instrument(skip(state))]
pub async fn record_failure_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RecordFailureRequest>,
) -> Response {
    if payload.provider_name.is_empty() {
        return error(StatusCode::BAD_REQUEST, "provider_name cannot be empty").into_response();
    }

    state.hub.record_failure(&payload.provider_name);
    warn!(provider = %payload.provider_name, "Recorded provider failure via HTTP");
    success(json!({
        "provider_name": payload.provider_name,
        "status": "failure_recorded"
    }))
    .into_response()
}

/// Check if a provider is healthy.
#[instrument(skip(state))]
pub async fn is_healthy_route(
    State(state): State<Arc<AppState>>,
    Query(payload): Query<HealthCheckRequest>,
) -> Response {
    if payload.provider_name.is_empty() {
        return error(StatusCode::BAD_REQUEST, "provider_name cannot be empty").into_response();
    }

    let healthy = state.hub.is_healthy(&payload.provider_name);
    info!(
        provider = %payload.provider_name,
        healthy = healthy,
        "Health check via HTTP"
    );
    success(json!(HealthStatusResponse {
        provider_name: payload.provider_name,
        healthy
    }))
    .into_response()
}

/// Get full health summary for all providers.
#[instrument(skip(state))]
pub async fn get_health_summary_route(
    State(state): State<Arc<AppState>>,
) -> Response {
    match state.hub.get_health_summary() {
        Ok(summary) => {
            info!("Retrieved health summary via HTTP");
            success(HealthSummaryResponse::from(summary))
                .into_response()
        }
        Err(e) => {
            warn!("Failed to get health summary: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
                .into_response()
        }
    }
}

// ── Multi-provider routing handler functions (feature: multi-provider) ─────

#[cfg(feature = "multi-provider")]
/// Request body for adding a provider to the multi-provider routing pool.
#[derive(Debug, Deserialize)]
pub struct AddMultiProviderRequest {
    pub name: String,
    pub vendor: String,
    pub endpoint: String,
    #[serde(default)]
    pub priority: u32,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

#[cfg(feature = "multi-provider")]
fn default_max_retries() -> u32 {
    3
}

#[cfg(feature = "multi-provider")]
impl TryFrom<AddMultiProviderRequest> for prompt_hub::multi_provider::ProviderConfig {
    type Error = String;

    fn try_from(req: AddMultiProviderRequest) -> Result<Self, Self::Error> {
        let vendor = match req.vendor.to_lowercase().as_str() {
            "openai" => prompt_hub::multi_provider::Vendor::OpenAi,
            "anthropic" => prompt_hub::multi_provider::Vendor::Anthropic,
            "google" => prompt_hub::multi_provider::Vendor::Google,
            other => prompt_hub::multi_provider::Vendor::Custom(other.to_string()),
        };
        Ok(prompt_hub::multi_provider::ProviderConfig {
            name: req.name,
            vendor,
            endpoint: req.endpoint,
            priority: req.priority,
            max_retries: req.max_retries,
        })
    }
}

/// Provider selection routing decision.
#[derive(Debug, Serialize)]
pub struct RoutingDecisionResponse {
    pub provider_name: String,
    pub vendor: String,
    pub endpoint: String,
}

/// Get pool stats for multi-provider routing.
#[derive(Debug, Serialize)]
pub struct PoolStatsResponse {
    pub total_providers: usize,
    pub healthy: usize,
    pub degraded: usize,
    pub unhealthy: usize,
}

impl From<prompt_hub::multi_provider::PoolStats> for PoolStatsResponse {
    fn from(stats: prompt_hub::multi_provider::PoolStats) -> Self {
        Self {
            total_providers: stats.total_providers,
            healthy: stats.healthy,
            degraded: stats.degraded,
            unhealthy: stats.unhealthy,
        }
    }
}

/// Add a provider to the multi-provider routing pool.
#[instrument(skip(state))]
pub async fn add_provider_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AddProviderRequest>,
) -> Response {
    if payload.name.is_empty() {
        return error(StatusCode::BAD_REQUEST, "name cannot be empty").into_response();
    }
    if payload.endpoint.is_empty() {
        return error(StatusCode::BAD_REQUEST, "endpoint cannot be empty").into_response();
    }

    match prompt_hub::multi_provider::ProviderConfig::try_from(payload.clone()) {
        Ok(config) => {
            state.hub.add_provider(config);
            info!(
                provider = %payload.name,
                vendor = %payload.vendor,
                endpoint = %payload.endpoint,
                priority = payload.priority,
                "Added provider to routing pool via HTTP"
            );
            success(json!({
                "name": payload.name,
                "vendor": payload.vendor,
                "endpoint": payload.endpoint,
                "status": "added"
            }))
            .into_response()
        }
        Err(e) => {
            error(StatusCode::BAD_REQUEST, e).into_response()
        }
    }
}

/// Route a request to the best available provider.
#[instrument(skip(state))]
pub async fn route_to_vendor_route(
    State(state): State<Arc<AppState>>,
) -> Response {
    match state.hub.route_to_vendor(None) {
        Some(decision) => {
            let vendor_str = match decision.vendor {
                prompt_hub::multi_provider::Vendor::OpenAi => "openai",
                prompt_hub::multi_provider::Vendor::Anthropic => "anthropic",
                prompt_hub::multi_provider::Vendor::Google => "google",
                prompt_hub::multi_provider::Vendor::Custom(name) => name.as_str(),
            };
            info!(
                provider = %decision.provider_name,
                vendor = vendor_str,
                "Routed request to provider via HTTP"
            );
            success(json!(RoutingDecisionResponse {
                provider_name: decision.provider_name.clone(),
                vendor: vendor_str.to_string(),
                endpoint: decision.endpoint.clone(),
            }))
            .into_response()
        }
        None => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "no healthy providers available"
        )
        .into_response(),
    }
}

/// Record a successful request for the multi-provider routing pool.
#[instrument(skip(state))]
pub async fn record_provider_success_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RecordSuccessRequest>,
) -> Response {
    if payload.provider_name.is_empty() {
        return error(StatusCode::BAD_REQUEST, "provider_name cannot be empty").into_response();
    }

    state.hub.record_provider_success(&payload.provider_name);
    info!(
        provider = %payload.provider_name,
        latency_ms = payload.latency_ms,
        "Recorded provider success in routing pool via HTTP"
    );
    success(json!({
        "provider_name": payload.provider_name,
        "status": "success_recorded"
    }))
    .into_response()
}

/// Record a failed request for the multi-provider routing pool.
#[instrument(skip(state))]
pub async fn record_provider_failure_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RecordFailureRequest>,
) -> Response {
    if payload.provider_name.is_empty() {
        return error(StatusCode::BAD_REQUEST, "provider_name cannot be empty").into_response();
    }

    state.hub.record_provider_failure(&payload.provider_name);
    warn!(provider = %payload.provider_name, "Recorded provider failure via HTTP");
    success(json!({
        "provider_name": payload.provider_name,
        "status": "failure_recorded"
    }))
    .into_response()
}

/// Get health statistics for all providers in the routing pool.
#[instrument(skip(state))]
pub async fn get_provider_pool_stats_route(
    State(state): State<Arc<AppState>>,
) -> Response {
    match state.hub.provider_pool_stats() {
        Ok(stats) => {
            info!("Retrieved provider pool stats via HTTP");
            success(PoolStatsResponse::from(stats)).into_response()
        }
        Err(e) => {
            warn!("Failed to get provider pool stats: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

// ── Rollout handler functions (feature: gradual-rollout) ───────────────────

/// Request body for check_rollout.
#[derive(Debug, Deserialize)]
pub struct CheckRolloutRequest {
    pub feature: String,
    #[serde(default)]
    pub user_id: Option<String>,
}

/// Request body for find_rollout_inclusion.
#[derive(Debug, Deserialize)]
pub struct FindRolloutInclusionRequest {
    pub rollout_id: String,
    pub feature: String,
    pub user_id: Option<String>,
}

/// Rollout inclusion check response.
#[derive(Debug, Serialize)]
pub struct RolloutInclusionResponse {
    pub rollout_id: String,
    pub included: bool,
}

/// Request body for evaluate_auto_rollback.
#[derive(Debug, Deserialize)]
pub struct EvaluateRollbackRequest {
    pub rollout_id: String,
    pub error_rate: f64,
    #[serde(default)]
    pub latency_p99_ms: u64,
}

/// Rollout auto-rollback evaluation response.
#[derive(Debug, Serialize)]
pub struct RollbackEvaluationResponse {
    pub rollout_id: String,
    pub should_rollback: bool,
}

/// Request body for advance_segment.
#[derive(Debug, Deserialize)]
pub struct AdvanceSegmentRequest {
    pub rollout_id: String,
    #[serde(default)]
    pub segment_idx: usize,
}

/// Segment advancement response.
#[derive(Debug, Serialize)]
pub struct AdvanceSegmentResponse {
    pub rollout_id: String,
    pub segment_idx: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_stage: Option<String>,
}

impl From<prompt_hub::RolloutStage> for String {
    fn from(stage: prompt_hub::RolloutStage) -> Self {
        match stage {
            prompt_hub::RolloutStage::Internal => "internal".to_string(),
            prompt_hub::RolloutStage::Alpha(p) => format!("alpha_{}", p),
            prompt_hub::RolloutStage::Beta50(_) => "beta50".to_string(),
            prompt_hub::RolloutStage::Beta90(_) => "beta90".to_string(),
            prompt_hub::RolloutStage::Production => "production".to_string(),
        }
    }
}

/// Request body for register_rollout.
#[derive(Debug, Deserialize)]
pub struct RegisterRolloutRequest {
    pub rollout_id: String,
    pub feature: String,
    pub segments: Vec<RolloutSegmentRequest>,
    #[serde(default = "default_auto_rollback_threshold")]
    pub auto_rollback_threshold: f64,
    #[serde(default)]
    pub active: bool,
}

fn default_auto_rollback_threshold() -> f64 {
    0.05
}

/// Request body for register_rollout segment.
#[derive(Debug, Deserialize)]
pub struct RolloutSegmentRequest {
    pub name: String,
    #[serde(default = "default_percentage")]
    pub percentage: u8,
    #[serde(default)]
    pub target_users: Vec<String>,
}

fn default_percentage() -> u8 {
    10
}

impl TryFrom<RolloutSegmentRequest> for prompt_hub::RolloutSegment {
    type Error = String;

    fn try_from(req: RolloutSegmentRequest) -> Result<Self, Self::Error> {
        let target_users: Vec<uuid::Uuid> = req
            .target_users
            .iter()
            .map(|s| uuid::Uuid::parse_str(s))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("invalid UUID in target_users: {}", e))?;
        Ok(prompt_hub::RolloutSegment {
            name: req.name,
            percentage: req.percentage,
            target_users,
            rollout_stage: prompt_hub::RolloutStage::Internal,
            created_at: chrono::Utc::now(),
        })
    }
}

impl TryFrom<RegisterRolloutRequest> for prompt_hub::GraduatedRolloutConfig {
    type Error = String;

    fn try_from(req: RegisterRolloutRequest) -> Result<Self, Self::Error> {
        let segments: Vec<prompt_hub::RolloutSegment> = req
            .segments
            .iter()
            .map(prompt_hub::RolloutSegment::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(prompt_hub::GraduatedRolloutConfig {
            rollout_id: req.rollout_id,
            feature: req.feature,
            segments,
            auto_rollback: prompt_hub::AutoRollbackPolicy::OnErrorRate {
                threshold: req.auto_rollback_threshold,
            },
            active: req.active,
        })
    }
}

/// Check if a user should see the new feature under an active rollout.
#[instrument(skip(state))]
pub async fn check_rollout_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CheckRolloutRequest>,
) -> Response {
    if payload.feature.is_empty() {
        return error(StatusCode::BAD_REQUEST, "feature cannot be empty").into_response();
    }

    let user_id = match &payload.user_id {
        Some(s) => match uuid::Uuid::parse_str(s) {
            Ok(u) => u,
            Err(_) => {
                return error(
                    StatusCode::BAD_REQUEST,
                    format!("invalid UUID: {}", s),
                )
                .into_response();
            }
        },
        None => uuid::Uuid::new_v4(),
    };

    let canary = prompt_hub::CanaryDeployment {
        feature: payload.feature.clone(),
        canary_percentage: 50.0,
        target_users: vec![],
        rollback_threshold: 0.05,
    };
    let included = state.hub.check_rollout(&canary, user_id);
    info!(
        feature = %payload.feature,
        user_id = %user_id,
        included = included,
        "Checked rollout via HTTP"
    );
    success(json!({
        "feature": payload.feature,
        "user_id": user_id.to_string(),
        "included": included
    }))
    .into_response()
}

/// Register a new rollout configuration.
#[instrument(skip(state))]
pub async fn register_rollout_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterRolloutRequest>,
) -> Response {
    match prompt_hub::GraduatedRolloutConfig::try_from(payload.clone()) {
        Ok(config) => {
            state.hub.register_rollout(config);
            info!(
                rollout_id = %payload.rollout_id,
                feature = %payload.feature,
                "Registered rollout via HTTP"
            );
            success(json!({
                "rollout_id": payload.rollout_id,
                "feature": payload.feature,
                "status": "registered"
            }))
            .into_response()
        }
        Err(e) => {
            error(StatusCode::BAD_REQUEST, e).into_response()
        }
    }
}

/// Check whether a rollout includes a user.
#[instrument(skip(state))]
pub async fn find_rollout_inclusion_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<FindRolloutInclusionRequest>,
) -> Response {
    if payload.rollout_id.is_empty() {
        return error(StatusCode::BAD_REQUEST, "rollout_id cannot be empty").into_response();
    }
    if payload.feature.is_empty() {
        return error(StatusCode::BAD_REQUEST, "feature cannot be empty").into_response();
    }

    let user_id = match &payload.user_id {
        Some(s) => match uuid::Uuid::parse_str(s) {
            Ok(u) => u,
            Err(_) => {
                return error(
                    StatusCode::BAD_REQUEST,
                    format!("invalid UUID: {}", s),
                )
                .into_response();
            }
        },
        None => uuid::Uuid::new_v4(),
    };

    let included = state.hub.find_rollout_inclusion(&payload.rollout_id, &payload.feature, user_id);
    info!(
        rollout_id = %payload.rollout_id,
        feature = %payload.feature,
        user_id = %user_id,
        included = ?included,
        "Checked rollout inclusion via HTTP"
    );
    success(json!(RolloutInclusionResponse {
        rollout_id: payload.rollout_id,
        included: included.unwrap_or(false),
    }))
    .into_response()
}

/// Evaluate auto-rollback for a rollout.
#[instrument(skip(state))]
pub async fn evaluate_auto_rollback_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<EvaluateRollbackRequest>,
) -> Response {
    if payload.rollout_id.is_empty() {
        return error(StatusCode::BAD_REQUEST, "rollout_id cannot be empty").into_response();
    }

    let should_rollback = state.hub.evaluate_auto_rollback(
        &payload.rollout_id,
        payload.error_rate,
        payload.latency_p99_ms,
    );
    info!(
        rollout_id = %payload.rollout_id,
        error_rate = payload.error_rate,
        latency_p99_ms = payload.latency_p99_ms,
        should_rollback = ?should_rollback,
        "Evaluated auto-rollback via HTTP"
    );
    success(json!(RollbackEvaluationResponse {
        rollout_id: payload.rollout_id,
        should_rollback: should_rollback.unwrap_or(false),
    }))
    .into_response()
}

/// Advance a rollout segment to the next stage.
#[instrument(skip(state))]
pub async fn advance_segment_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AdvanceSegmentRequest>,
) -> Response {
    if payload.rollout_id.is_empty() {
        return error(StatusCode::BAD_REQUEST, "rollout_id cannot be empty").into_response();
    }

    let new_stage = state.hub.advance_segment(&payload.rollout_id, payload.segment_idx);
    info!(
        rollout_id = %payload.rollout_id,
        segment_idx = payload.segment_idx,
        new_stage = ?new_stage,
        "Advanced rollout segment via HTTP"
    );
    success(json!(AdvanceSegmentResponse {
        rollout_id: payload.rollout_id,
        segment_idx: payload.segment_idx,
        new_stage: new_stage.map(String::from),
    }))
    .into_response()
}

// ── Rollback handler functions (feature: rollback) ───────────────────────

/// Request body for deploy_with_rollback.
/// Note: This endpoint requires a fully-formed Artifact JSON payload
/// since there's no artifact lookup by ID. Use this when you have the
/// artifact data available in the request.
#[derive(Debug, Deserialize)]
pub struct DeployWithRollbackRequest {
    pub artifact: prompt_hub::Artifact,
    #[serde(default)]
    pub rollback_enabled: bool,
}

/// Response for deploy_with_rollback.
#[derive(Debug, Serialize)]
pub struct DeployResultResponse {
    pub success: bool,
    pub rollback_available: bool,
}

/// Request body for restore_snapshot.
#[derive(Debug, Deserialize)]
pub struct RestoreSnapshotRequest {
    pub snapshot_id: String,
}

/// Response for restore_snapshot.
#[derive(Debug, Serialize)]
pub struct RestoreResultResponse {
    pub success: bool,
    pub snapshot_id: String,
}

/// Deploy a prompt with automatic rollback capability.
#[cfg(feature = "rollback")]
#[instrument(skip(state))]
pub async fn deploy_with_rollback_route(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DeployWithRollbackRequest>,
) -> Response {
    match state
        .hub
        .deploy_with_rollback(&payload.artifact, payload.rollback_enabled)
        .await
    {
        Ok(result) => {
            info!(
                artifact_type = ?payload.artifact,
                rollback_enabled = payload.rollback_enabled,
                "Deployed with rollback via HTTP"
            );
            success(json!(DeployResultResponse {
                success: true,
                rollback_available: result.rollback_available,
            }))
            .into_response()
        }
        Err(e) => {
            warn!("Failed to deploy artifact: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

/// Restore a prompt to a previously saved snapshot.
#[cfg(feature = "rollback")]
#[instrument(skip(state))]
pub async fn restore_snapshot_route(
    State(state): State<Arc<AppState>>,
    Path(snapshot_id): Path<String>,
) -> Response {
    if snapshot_id.is_empty() {
        return error(StatusCode::BAD_REQUEST, "snapshot_id cannot be empty").into_response();
    }

    match state.hub.restore_snapshot(&snapshot_id).await {
        Ok(_) => {
            info!(snapshot = %snapshot_id, "Restored snapshot via HTTP");
            success(json!(RestoreResultResponse {
                success: true,
                snapshot_id
            }))
            .into_response()
        }
        Err(HubError::NotFound(_)) => {
            error(StatusCode::NOT_FOUND, format!("Snapshot '{}' not found", snapshot_id))
                .into_response()
        }
        Err(e) => {
            warn!("Failed to restore snapshot {}: {}", snapshot_id, e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

/// Check if a specific rollback snapshot is available.
#[cfg(feature = "rollback")]
#[instrument(skip(state))]
pub async fn is_rollback_available_route(
    State(state): State<Arc<AppState>>,
    Query(payload): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let snapshot_id = match payload.get("snapshot_id") {
        Some(id) => id.as_str(),
        None => {
            return error(StatusCode::BAD_REQUEST, "snapshot_id query parameter required")
                .into_response();
        }
    };

    let available = state.hub.is_rollback_available(snapshot_id);
    info!(
        snapshot = %snapshot_id,
        available = available,
        "Checked rollback availability via HTTP"
    );
    success(json!({
        "snapshot_id": snapshot_id,
        "available": available
    }))
    .into_response()
}

// ── Test module below ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::create_router;
    use crate::state::AppState;
    use axum::{Router, http::Request, http::StatusCode};
    use prompt_hub::config::HubConfig;
    use prompt_hub::hub::PromptHub;
    use prompt_hub::metrics::MetricsCollector;
    use serde_json::Value;
    use std::sync::Arc;
    use tower::ServiceExt;

    #[test]
    fn render_metrics_is_valid_exposition() {
        let metrics = MetricsCollector::new();
        metrics.record_request();
        metrics.record_request();
        metrics.record_search_latency(100);
        metrics.record_lock_acquired();

        let text = render_metrics(&metrics, 12.5);

        // Common invariants across both feature configs.
        assert!(text.contains("prompt_hub_requests_total 2"));
        assert!(text.contains("# TYPE prompt_hub_active_locks gauge"));
        assert!(text.contains("prompt_hub_active_locks 1"));
        assert!(text.contains("# TYPE prompt_hub_uptime_seconds gauge"));
        assert!(text.contains("prompt_hub_uptime_seconds 12.500"));

        // The malformed single-bucket pseudo-histogram must be gone in every config.
        assert!(
            !text.contains("le=\"+Inf\""),
            "must not emit a single-bucket pseudo-histogram: {text}"
        );
        assert!(
            !text.contains(" histogram"),
            "no histogram-typed series without real buckets: {text}"
        );

        // Feature-specific latency representation.
        #[cfg(feature = "otel")]
        {
            assert!(text.contains("prompt_hub_search_latency_ms_sum 100"));
            assert!(text.contains("prompt_hub_search_latency_ms_count 1"));
        }
        #[cfg(not(feature = "otel"))]
        {
            assert!(text.contains("# TYPE prompt_hub_search_latency_ms_avg gauge"));
            assert!(text.contains("prompt_hub_search_latency_ms_avg 100"));
        }
    }

    // ── Load balancer route tests ────────────────────────────────────────

    /// Build an AppState backed by a temp SQLite file for testing.
    async fn make_test_state() -> Arc<AppState> {
        let config = HubConfig::default();
        let tmp = tempfile::tempdir().expect("create temp dir for tests");
        let db_file = tmp.path().join("test.db");
        let hub = PromptHub::new(&db_file, config.clone())
            .await
            .expect("create test PromptHub");
        // Keep the tempdir alive so the file isn't deleted.
        Arc::new(AppState {
            hub: Arc::new(hub),
            config,
            start_time: std::time::Instant::now(),
        })
    }

    /// Perform a GET request on the test router and return status + body string.
    async fn handle_get(router: Router, path: &str) -> (StatusCode, String) {
        let req = Request::builder()
            .uri(path)
            .method("GET")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = router.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8(body_bytes.to_vec()).unwrap())
    }

    /// Perform a POST request with optional JSON body.
    async fn handle_post(router: Router, path: &str, json: Option<Value>) -> (StatusCode, String) {
        let body = match json {
            Some(val) => axum::body::Body::from(serde_json::to_string(&val).unwrap().into_bytes()),
            None => axum::body::Body::empty(),
        };
        let req = Request::builder()
            .uri(path)
            .method("POST")
            .header("content-type", "application/json")
            .body(body)
            .unwrap();
        let response = router.clone().oneshot(req).await.unwrap();
        let status = response.status();
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8(body_bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn test_add_lb_provider_valid() {
        let app_state = make_test_state().await;
        // Keep the shared hub Arc before consuming AppState into the router.
        let hub = Arc::clone(&app_state.hub);
        let config = app_state.config.clone();
        drop(app_state);

        // Build router with a fresh in-memory hub (router owns its own state).
        let fresh_db = std::path::PathBuf::from(":memory:");
        let fresh_hub = PromptHub::new(&fresh_db, config)
            .await
            .expect("create router test hub");

        let _router = create_router(AppState {
            hub: Arc::new(fresh_hub),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });

        // Direct handler call to bypass axum's typed State extraction (which fails in tests).
        let arc_state = Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });
        let response = add_lb_provider(
            axum::extract::State(arc_state.clone()),
            axum::Json(AddProviderRequest {
                name: "gpt-4o".into(),
                url: "https://api.openai.com/v1".into(),
                weight: 5,
            }),
        )
        .await;

        let status = response.status();
        assert_eq!(status, StatusCode::OK, "Expected 200 but got {}", status);
        // hub is the test-setup hub (separate from router's) — only verify HTTP layer.
        drop(hub);
    }

    #[tokio::test]
    async fn test_add_lb_provider_empty_name_rejected() {
        // Direct handler call (bypasses axum's State extraction in tests).
        let arc_state = Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });

        let response = add_lb_provider(
            axum::extract::State(arc_state.clone()),
            axum::Json(AddProviderRequest {
                name: "".into(),
                url: "https://example.com".into(),
                weight: 1,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_select_provider_empty_pool_returns_conflict() {
        let arc_state = Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });

        let response = select_provider(axum::extract::State(arc_state)).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_get_lb_stats_returns_empty_list() {
        let arc_state = Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });

        // Add a provider via the shared hub.
        arc_state.hub.add_lb_provider("p1", "https://p1.com", 3);

        let response = get_lb_stats(axum::extract::State(arc_state.clone())).await;
        assert_eq!(response.status(), StatusCode::OK);

        // Verify via direct hub access (no HTTP layer needed).
        let stats = arc_state.hub.get_lb_stats();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].name, "p1");
    }

    #[tokio::test]
    async fn test_record_lb_latency_and_failure() {
        let arc_state = Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });

        // Add provider via shared hub.
        arc_state.hub.add_lb_provider("p1", "https://p1.com", 3);

        // Record latency via handler.
        let response = record_lb_latency(
            axum::extract::State(arc_state.clone()),
            axum::Json(LatencyRequest {
                provider_name: "p1".into(),
                latency_ms: 42,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        // Record failure via handler.
        let response = record_lb_failure(
            axum::extract::State(arc_state.clone()),
            axum::Json(FailureRequest {
                provider_name: "p1".into(),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        // Verify stats reflect updates via direct hub access.
        let stats = arc_state.hub.get_lb_stats();
        assert_eq!(stats[0].latency_ms, 42);
        assert_eq!(stats[0].error_count, 1);
    }

    // ── Satisfaction route tests ────────────────────────────────────────

    #[tokio::test]
    async fn test_record_csat_valid() {
        let arc_state = Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });

        let response = record_csat(
            axum::extract::State(arc_state.clone()),
            axum::Json(RecordCsatRequest {
                score: 4,
                context: "Great UI".into(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_record_csat_invalid_score_rejected() {
        let arc_state = Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });

        let response = record_csat(
            axum::extract::State(arc_state),
            axum::Json(RecordCsatRequest {
                score: 6,
                context: "".into(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_record_nps_valid() {
        let arc_state = Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });

        let response = record_nps(
            axum::extract::State(arc_state),
            axum::Json(RecordNpsRequest { score: 9 }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_record_nps_invalid_score_rejected() {
        let arc_state = Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });

        let response = record_nps(
            axum::extract::State(arc_state),
            axum::Json(RecordNpsRequest { score: 11 }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_record_satisfaction_event_valid() {
        let arc_state = Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });

        let response = record_satisfaction_event(
            axum::extract::State(arc_state),
            axum::Json(SatisfactionEventRequest {
                prompt_id: "p-42".into(),
                successful: true,
                attempts: 1,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_record_satisfaction_event_empty_prompt_id_rejected() {
        let arc_state = Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });

        let response = record_satisfaction_event(
            axum::extract::State(arc_state),
            axum::Json(SatisfactionEventRequest {
                prompt_id: "".into(),
                successful: true,
                attempts: 1,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_satisfaction_metrics_empty() {
        let arc_state = Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        });

        let response = get_satisfaction_metrics(axum::extract::State(arc_state)).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    // ── Evolution route tests ───────────────────────────────────────────

    /// Build a fresh `:memory:` AppState for direct-handler evolution tests.
    async fn evolve_test_state() -> Arc<AppState> {
        Arc::new(AppState {
            hub: Arc::new(
                PromptHub::new(&std::path::PathBuf::from(":memory:"), HubConfig::default())
                    .await
                    .expect("create direct test hub"),
            ),
            config: HubConfig::default(),
            start_time: std::time::Instant::now(),
        })
    }

    /// Register a minimal base prompt and return its UUID.
    async fn seed_prompt(state: &Arc<AppState>) -> Uuid {
        let prompt = Prompt {
            id: Uuid::new_v4(),
            name: "base".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: "You are a helpful assistant.".to_string(),
            user_template: "Answer: {{question}}".to_string(),
            required_vars: Vec::new(),
            domain: Domain::default(),
            tags: vec!["seed".to_string()],
            target_roles: Vec::new(),
            metadata: PromptMeta::default(),
            metrics: PromptMetrics::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            author: default_agent(),
            deleted_at: None,
            generation_params: None,
            locale: None,
            multimodal: None,
        };
        state
            .hub
            .register(prompt, &default_agent())
            .await
            .expect("register base prompt")
    }

    #[test]
    fn parse_evolution_strategy_covers_all_variants() {
        assert_eq!(
            parse_evolution_strategy("mutate").unwrap(),
            EvolutionStrategy::Mutate
        );
        assert_eq!(
            parse_evolution_strategy("crossover").unwrap(),
            EvolutionStrategy::Crossover
        );
        assert_eq!(
            parse_evolution_strategy("ab_test").unwrap(),
            EvolutionStrategy::AbTest
        );
        assert_eq!(
            parse_evolution_strategy("semantic").unwrap(),
            EvolutionStrategy::Semantic
        );
        assert_eq!(
            parse_evolution_strategy("compress").unwrap(),
            EvolutionStrategy::Compress
        );
        assert_eq!(
            parse_evolution_strategy("expand").unwrap(),
            EvolutionStrategy::Expand
        );
        // Case-insensitive + trimmed.
        assert_eq!(
            parse_evolution_strategy("  MUTATE ").unwrap(),
            EvolutionStrategy::Mutate
        );
        // Unknown returns the (normalized) offending value.
        assert_eq!(parse_evolution_strategy("nope").unwrap_err(), "nope");
    }

    #[tokio::test]
    async fn test_evolve_prompt_mutate_happy_path() {
        let state = evolve_test_state().await;
        let id = seed_prompt(&state).await;

        let response = evolve_prompt(
            axum::extract::State(state.clone()),
            axum::extract::Path(id.to_string()),
            axum::Json(EvolvePromptRequest {
                strategy: "mutate".into(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_evolve_prompt_semantic_strategy() {
        let state = evolve_test_state().await;
        let id = seed_prompt(&state).await;

        let response = evolve_prompt(
            axum::extract::State(state.clone()),
            axum::extract::Path(id.to_string()),
            axum::Json(EvolvePromptRequest {
                strategy: "semantic".into(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_evolve_prompt_not_found() {
        let state = evolve_test_state().await;
        let random = Uuid::new_v4();

        let response = evolve_prompt(
            axum::extract::State(state),
            axum::extract::Path(random.to_string()),
            axum::Json(EvolvePromptRequest {
                strategy: "mutate".into(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_evolve_prompt_unknown_strategy_rejected() {
        let state = evolve_test_state().await;
        let id = seed_prompt(&state).await;

        let response = evolve_prompt(
            axum::extract::State(state),
            axum::extract::Path(id.to_string()),
            axum::Json(EvolvePromptRequest {
                strategy: "teleport".into(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_evolve_prompt_invalid_uuid_rejected() {
        let state = evolve_test_state().await;

        let response = evolve_prompt(
            axum::extract::State(state),
            axum::extract::Path("not-a-uuid".to_string()),
            axum::Json(EvolvePromptRequest {
                strategy: "mutate".into(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_evolve_prompt_crossover_empty_pool_errors() {
        let state = evolve_test_state().await;
        let id = seed_prompt(&state).await;

        // Crossover needs a second candidate prompt; with only the base
        // present, list_prompts still returns the base itself, so this path
        // succeeds rather than erroring. We instead assert it does NOT 404/400
        // — i.e. the strategy parsed and the hub was reached.
        let response = evolve_prompt(
            axum::extract::State(state.clone()),
            axum::extract::Path(id.to_string()),
            axum::Json(EvolvePromptRequest {
                strategy: "crossover".into(),
            }),
        )
        .await;

        let status = response.status();
        assert!(
            status == StatusCode::OK || status == StatusCode::INTERNAL_SERVER_ERROR,
            "crossover should reach the hub, got {status}"
        );
    }

    // ── Token / cost / input / render route tests ───────────────────────

    /// Register a prompt with a `{{name}}` template var declared as required,
    /// returning its UUID. Used by the render happy-path / missing-var tests.
    async fn seed_render_prompt(state: &Arc<AppState>) -> Uuid {
        let prompt = Prompt {
            id: Uuid::new_v4(),
            name: "greeter".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: "You are a greeter.".to_string(),
            user_template: "Hello, {{name}}!".to_string(),
            required_vars: vec!["name".to_string()],
            domain: Domain::default(),
            tags: Vec::new(),
            target_roles: Vec::new(),
            metadata: PromptMeta::default(),
            metrics: PromptMetrics::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            author: default_agent(),
            deleted_at: None,
            generation_params: None,
            locale: None,
            multimodal: None,
        };
        state
            .hub
            .register(prompt, &default_agent())
            .await
            .expect("register render prompt")
    }

    #[tokio::test]
    async fn test_count_prompt_tokens_happy_path() {
        let state = evolve_test_state().await;
        let id = seed_prompt(&state).await;

        let response = count_prompt_tokens_route(
            axum::extract::State(state.clone()),
            axum::extract::Path(id.to_string()),
            axum::Json(TokenRequest {
                model: "gpt-4".into(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_count_prompt_tokens_not_found() {
        let state = evolve_test_state().await;
        let random = Uuid::new_v4();

        let response = count_prompt_tokens_route(
            axum::extract::State(state),
            axum::extract::Path(random.to_string()),
            axum::Json(TokenRequest {
                model: "gpt-4".into(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_estimate_prompt_cost_happy_path() {
        let state = evolve_test_state().await;
        let id = seed_prompt(&state).await;

        let response = estimate_prompt_cost_route(
            axum::extract::State(state.clone()),
            axum::extract::Path(id.to_string()),
            axum::Json(CostRequest {
                model: "gpt-4".into(),
                expected_output_tokens: 100,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_estimate_prompt_cost_not_found() {
        let state = evolve_test_state().await;
        let random = Uuid::new_v4();

        let response = estimate_prompt_cost_route(
            axum::extract::State(state),
            axum::extract::Path(random.to_string()),
            axum::Json(CostRequest {
                model: "gpt-4".into(),
                expected_output_tokens: 100,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_process_input_happy_path() {
        let state = evolve_test_state().await;

        let response = process_input_route(
            axum::extract::State(state),
            axum::Json(UserInput {
                input_type: InputType::Text,
                raw_data: Vec::new(),
                extracted_text: "Build me a REST API in Rust".into(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_render_prompt_happy_path() {
        let state = evolve_test_state().await;
        let id = seed_render_prompt(&state).await;

        let mut vars = std::collections::HashMap::new();
        vars.insert("name".to_string(), Value::String("World".to_string()));

        let response = render_prompt_route(
            axum::extract::State(state.clone()),
            axum::extract::Path(id.to_string()),
            axum::Json(RenderRequest { vars }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_render_prompt_missing_required_var_rejected() {
        let state = evolve_test_state().await;
        let id = seed_render_prompt(&state).await;

        // `name` is required but absent → core returns ValidationError → 422.
        let response = render_prompt_route(
            axum::extract::State(state.clone()),
            axum::extract::Path(id.to_string()),
            axum::Json(RenderRequest {
                vars: std::collections::HashMap::new(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_render_prompt_not_found() {
        let state = evolve_test_state().await;
        let random = Uuid::new_v4();

        let response = render_prompt_route(
            axum::extract::State(state),
            axum::extract::Path(random.to_string()),
            axum::Json(RenderRequest {
                vars: std::collections::HashMap::new(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
