#![forbid(unsafe_code)]

use axum::{
    Router,
    middleware::from_fn,
    routing::{delete, get, post},
};
use std::sync::Arc;
use std::time::Duration;
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::compression::CompressionLayer;
use tower_http::timeout::TimeoutLayer;
use tracing::instrument;

use crate::middleware;
use crate::routes;
use crate::state::AppState;

/// Build the axum router with all routes, state, and middleware layers.
#[instrument(skip(state))]
pub fn create_router(state: AppState) -> Router {
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(100)
            .burst_size(50)
            .use_headers()
            .finish()
            .expect("valid governor config"),
    );

    let state_arc = Arc::new(state);

    Router::new()
        // Prompt CRUD
        .route("/api/v1/prompts", post(routes::register_prompt))
        .route("/api/v1/prompts", get(routes::list_prompts))
        .route("/api/v1/prompts/{id}", get(routes::get_prompt))
        .route("/api/v1/prompts/search", get(routes::search_prompts))
        // Lock management
        .route("/api/v1/prompts/{id}/lock", post(routes::lock_prompt))
        .route("/api/v1/prompts/{id}/lock", delete(routes::unlock_prompt))
        // Audit
        .route("/api/v1/prompts/{id}/audit", get(routes::audit_trail))
        // Swarm
        .route("/api/v1/swarm/bundle", get(routes::generate_bundle))
        // Vibe coding — natural language → deliverable (feature: vibe)
        .route("/api/v1/vibe/code", post(routes::vibe_code))
        // Health (Kubernetes probes)
        .route("/health", get(routes::health_check))
        .route("/ready", get(routes::ready_check))
        .route("/live", get(routes::live_check))
        // Metrics
        .route("/metrics", get(routes::prometheus_metrics))
        // OpenAPI docs
        .route("/openapi.json", get(routes::openapi_json))
        .route("/docs", get(routes::swagger_ui))
        // State
        .with_state(state_arc)
        // Middleware — applied directly on the Router (not bundled in a
        // ServiceBuilder) so the `from_fn` layers satisfy axum's Service bounds.
        // axum applies layers bottom-up: the LAST `.layer` is the OUTERMOST, so
        // this order preserves outer→inner = Compression, Timeout, Governor,
        // error_handler, request_timing, request_id, cors, trace.
        .layer(middleware::create_trace_layer())
        .layer(middleware::create_cors_layer())
        .layer(middleware::create_request_id_layer())
        .layer(from_fn(middleware::request_timing))
        .layer(from_fn(middleware::error_handler))
        .layer(GovernorLayer {
            config: governor_conf,
        })
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(CompressionLayer::new())
}
