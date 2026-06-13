#![forbid(unsafe_code)]

#[cfg(feature = "budget")]
use axum::routing::put;
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

    // Build the router: all routes first (no State yet), then apply State once.
    let router = Router::new()
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
        .route("/docs", get(routes::swagger_ui));

    // Vibe coding — natural language → deliverable (feature: vibe)
    #[cfg(feature = "vibe")]
    let router = router.route("/api/v1/vibe/code", post(routes::vibe_code));

    // Budget tracking (feature: budget)
    #[cfg(feature = "budget")]
    let router = router
        .route("/api/v1/budget/spend", post(routes::budget_record_spend))
        .route("/api/v1/budget/status", get(routes::budget_status))
        .route("/api/v1/budget/budget", put(routes::set_monthly_budget))
        .route(
            "/api/v1/budget/config/load",
            post(routes::load_budget_config),
        )
        .route(
            "/api/v1/budget/config/save/{org_id}",
            get(routes::save_budget_config),
        )
        .route("/api/v1/budget/reset", post(routes::reset_budget_period));

    // Load balancer routes (always-on)
    let router = router
        .route("/api/v1/lb/providers", post(routes::add_lb_provider))
        .route("/api/v1/lb/select", post(routes::select_provider))
        .route("/api/v1/lb/latency", post(routes::record_lb_latency))
        .route("/api/v1/lb/failure", post(routes::record_lb_failure))
        .route("/api/v1/lb/stats", get(routes::get_lb_stats));

    // Satisfaction routes (always-on)
    let router = router
        .route("/api/v1/satisfaction/csat", post(routes::record_csat))
        .route("/api/v1/satisfaction/nps", post(routes::record_nps))
        .route(
            "/api/v1/satisfaction/events",
            post(routes::record_satisfaction_event),
        )
        .route(
            "/api/v1/satisfaction/metrics",
            get(routes::get_satisfaction_metrics),
        );

    // Apply State BEFORE middleware — required for handlers using `State<T>` extractors.
    router
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
