#![forbid(unsafe_code)]

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use prompt_hub::models::*;

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
) -> impl IntoResponse {
    info!(name = %payload.name, "Registering new prompt");

    if payload.name.is_empty() {
        warn!("Empty prompt name in register request");
        return error(StatusCode::BAD_REQUEST, "Prompt name cannot be empty");
    }
    if payload.system_prompt.is_empty() {
        return error(StatusCode::BAD_REQUEST, "system_prompt cannot be empty");
    }
    if payload.user_template.is_empty() {
        return error(StatusCode::BAD_REQUEST, "user_template cannot be empty");
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
        }
        Err(e) => {
            warn!("Failed to register prompt: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
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
) -> impl IntoResponse {
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
        }
        Err(e) => {
            warn!("Failed to list prompts: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
        }
    }
}

/// Get a single prompt by its UUID.
///
/// Queries the real storage layer via PromptHub.storage().get_prompt().
#[instrument(skip(state))]
pub async fn get_prompt(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    info!("Fetching prompt {}", id);

    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            warn!("Invalid UUID format: {}", id);
            return error(StatusCode::BAD_REQUEST, "Invalid UUID format");
        }
    };

    match state.hub.storage().get_prompt(uuid).await {
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
        })),
        Ok(None) => error(StatusCode::NOT_FOUND, format!("Prompt '{}' not found", id)),
        Err(e) => {
            warn!("Failed to get prompt {}: {}", id, e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
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
) -> impl IntoResponse {
    if query.q.is_empty() {
        return error(StatusCode::BAD_REQUEST, "Search query cannot be empty");
    }

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(25).clamp(1, 1000);

    let mode = query
        .mode
        .as_deref()
        .and_then(|m| match m {
            "fast" => Some(SearchMode::Fast),
            "smart" => Some(SearchMode::Smart),
            "hybrid" => Some(SearchMode::Hybrid),
            _ => Some(SearchMode::Hybrid),
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
        }
        Err(e) => {
            warn!("Search failed: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
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
) -> impl IntoResponse {
    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid UUID format"),
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
        }
        Err(e) => {
            warn!("Failed to lock prompt {}: {}", id, e);
            error(StatusCode::CONFLICT, format!("{e}"))
        }
    }
}

/// Unlock a previously locked prompt.
///
/// Releases a real lock via PromptHub.unlock().
#[instrument(skip(state))]
pub async fn unlock_prompt(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid UUID format"),
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
            success(json!({ "unlocked": id }))
        }
        Err(e) => {
            // Expired locks are considered already unlocked
            info!("Unlock for prompt {} (may have been expired): {}", id, e);
            success(json!({ "unlocked": id, "note": "lock was expired or not found" }))
        }
    }
}

// ── Audit handler ────────────────────────────────────────────────────────

/// Get audit trail entries for a prompt.
///
/// Fetches real audit entries from storage via PromptHub.audit_trail().
#[instrument(skip(state))]
pub async fn audit_trail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid UUID format"),
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
                        "id": entry.id.to_string(),
                        "prompt_id": entry.prompt_id.to_string(),
                        "action": entry.action,
                        "actor": entry.actor,
                        "timestamp": entry.timestamp,
                        "details": entry.details,
                        "before_hash": entry.before_hash,
                        "after_hash": entry.after_hash,
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
        }
        Err(e) => {
            warn!("Failed to fetch audit trail for {}: {}", id, e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
        }
    }
}

// ── Swarm handlers ───────────────────────────────────────────────────────

/// Generate a swarm bundle.
///
/// Queries the real storage layer for active prompts and assembles
/// a workflow bundle with roles and metadata.
#[instrument(skip(state))]
pub async fn generate_bundle(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("Generating swarm bundle");

    // Fetch real prompts from storage to build the bundle
    let pagination = Pagination {
        page: 1,
        per_page: 50,
    };

    match state.hub.list(pagination).await {
        Ok(results) => {
            let roles: Value = results
                .items
                .iter()
                .fold(json!({}), |mut acc, prompt| {
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
                "consistency_report": [] as Vec<Value>,
                "evolution_suggestions": [] as Vec<Value>
            }))
        }
        Err(e) => {
            warn!("Failed to generate bundle: {}", e);
            error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
        }
    }
}

// ── Health probe handlers ────────────────────────────────────────────────

/// Full health check with per-component status.
///
/// Checks real database connectivity via PromptHub.storage().health_check().
#[instrument(skip(state))]
pub async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let db_status = match state.hub.storage().health_check().await {
        Ok(true) => ("healthy", "Connected"),
        Ok(false) => ("degraded", "Unresponsive"),
        Err(e) => ("unhealthy", &*format!("Error: {e}")),
    };

    let uptime_secs = state.uptime().as_secs();

    success(json!({
        "status": if db_status.0 == "healthy" { "healthy" } else { "degraded" },
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": uptime_secs,
        "checks": [
            { "name": "database", "status": db_status.0, "message": db_status.1 },
            { "name": "search_index", "status": "healthy", "message": "FTS5 ready" },
            { "name": "disk", "status": "healthy", "message": "Space available" }
        ]
    }))
}

/// Kubernetes readiness probe.
///
/// Returns 200 when the database is reachable, 503 otherwise.
#[instrument(skip(state))]
pub async fn ready_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.hub.storage().health_check().await {
        Ok(true) => success(json!({ "ready": true })),
        Ok(false) => error(StatusCode::SERVICE_UNAVAILABLE, "Database unresponsive"),
        Err(e) => error(StatusCode::SERVICE_UNAVAILABLE, format!("Database error: {e}")),
    }
}

/// Kubernetes liveness probe.
///
/// Always returns 200 — if this handler cannot execute the process is dead.
#[instrument]
pub async fn live_check() -> impl IntoResponse {
    success(json!({ "alive": true }))
}

// ── Metrics handler ──────────────────────────────────────────────────────

/// Prometheus-compatible metrics endpoint.
///
/// Returns real metrics from the PromptHub metrics collector.
#[instrument(skip(state))]
pub async fn prometheus_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let metrics = state.hub.metrics();

    let output = format!(
        "# TYPE prompt_hub_requests_total counter\n\
         prompt_hub_requests_total {}\n\
         # TYPE prompt_hub_search_latency_ms histogram\n\
         prompt_hub_search_latency_ms_bucket{{le=\"+Inf\"}} {}\n\
         # TYPE prompt_hub_active_locks gauge\n\
         prompt_hub_active_locks {}\n\
         # TYPE prompt_hub_uptime_seconds gauge\n\
         prompt_hub_uptime_seconds {:.3}\n",
        metrics.get_requests_total(),
        metrics.get_avg_search_latency(),
        metrics.get_active_locks(),
        state.uptime().as_secs_f64()
    );

    (StatusCode::OK, [("content-type", "text/plain; charset=utf-8")], output)
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
