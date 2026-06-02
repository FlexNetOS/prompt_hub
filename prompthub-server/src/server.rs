#![forbid(unsafe_code)]

use axum::{
    middleware::from_fn,
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;
use std::time::Duration;
use tower::{timeout::TimeoutLayer, ServiceBuilder};
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::compression::CompressionLayer;
use tracing::{info, instrument};

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

    // Middleware stack (applied outer -> inner)
    let middleware = ServiceBuilder::new()
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        .layer(GovernorLayer::new(&governor_conf))
        .layer(from_fn(middleware::error_handler))
        .layer(from_fn(middleware::request_timing))
        .layer(middleware::create_request_id_layer())
        .layer(middleware::create_cors_layer())
        .layer(middleware::create_trace_layer());

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
        .layer(middleware)
}
