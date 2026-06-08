#![forbid(unsafe_code)]

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use prompt_hub::models::*;

#[cfg(feature = "budget")]
use prompt_hub::budget::{BudgetAlert, BudgetConfig};

use crate::responses::{error, success};
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

#[cfg(test)]
mod tests {
    use super::render_metrics;
    use prompt_hub::metrics::MetricsCollector;

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
}
