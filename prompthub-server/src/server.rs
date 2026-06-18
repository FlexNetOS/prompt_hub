#![forbid(unsafe_code)]

#[cfg(feature = "budget")]
use axum::routing::put;
use axum::{
    Router,
    middleware::from_fn,
    routing::{delete, get, patch, post},
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
        .route("/api/v1/prompts/{id}", patch(routes::update_prompt))
        .route("/api/v1/prompts/get", get(routes::get_prompt_route))
        .route("/api/v1/prompts/{id}/evolve", post(routes::evolve_prompt));

    // Rollback is feature-gated in the handler module.
    #[cfg(feature = "rollback")]
    let router = router.route(
        "/api/v1/prompts/{id}/rollback",
        post(routes::rollback_prompt),
    );

    let router = router
        .route(
            "/api/v1/prompts/{id}/transfer",
            post(routes::transfer_ownership),
        )
        .route("/api/v1/seed", post(routes::seed_defaults_route))
        .route("/api/v1/template/lint", post(routes::lint_template_route))
        .route(
            "/api/v1/prompts/{id}/tokens",
            post(routes::count_prompt_tokens_route),
        )
        .route(
            "/api/v1/prompts/{id}/cost",
            post(routes::estimate_prompt_cost_route),
        )
        .route(
            "/api/v1/prompts/{id}/render",
            post(routes::render_prompt_route),
        )
        .route("/api/v1/input/process", post(routes::process_input_route))
        .route("/api/v1/prompts/search", get(routes::search_prompts))
        // Lock management
        .route("/api/v1/prompts/{id}/lock", post(routes::lock_prompt))
        .route("/api/v1/prompts/{id}/lock", delete(routes::unlock_prompt))
        // Audit
        .route("/api/v1/prompts/{id}/audit", get(routes::audit_trail))
        .route("/api/v1/audit/hash", post(routes::compute_audit_hash_route))
        .route(
            "/api/v1/audit/verify",
            post(routes::verify_audit_integrity_route),
        )
        .route(
            "/api/v1/audit/soc2/summary",
            post(routes::soc2_evidence_summary_route),
        )
        .route(
            "/api/v1/audit/soc2/validate",
            post(routes::validate_soc2_schema_route),
        )
        .route(
            "/api/v1/audit/anonymize",
            post(routes::anonymize_audit_entry_route),
        )
        .route("/api/v1/diff/compute", post(routes::compute_diff_route))
        .route("/api/v1/diff/summarize", post(routes::summarize_diff_route))
        .route("/api/v1/diff/identical", post(routes::is_identical_route))
        .route(
            "/api/v1/diff/unified",
            post(routes::format_unified_diff_route),
        )
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

    // Context gathering (base route always-on; smart routes require gather)
    let router = router.route("/api/v1/context/gather", post(routes::gather_context_route));

    #[cfg(feature = "gather")]
    let router = router
        .route(
            "/api/v1/context/gather/smart",
            post(routes::gather_context_smart_route),
        )
        .route(
            "/api/v1/context/files",
            post(routes::collect_relevant_files_route),
        )
        .route(
            "/api/v1/context/patterns",
            post(routes::extract_patterns_route),
        );

    // Lineage routes (always-on)
    let router = router
        .route(
            "/api/v1/lineage/ancestry/{version_id}",
            get(routes::get_lineage_ancestry_route),
        )
        .route(
            "/api/v1/lineage/forks",
            get(routes::detect_lineage_forks_route),
        )
        .route(
            "/api/v1/lineage/descendants/{version_id}",
            get(routes::get_lineage_descendants_route),
        )
        .route(
            "/api/v1/lineage/tree/{version_id}",
            get(routes::build_lineage_tree_route),
        )
        .route(
            "/api/v1/lineage/count",
            get(routes::lineage_node_count_route),
        )
        .route("/api/v1/lineage/roots", get(routes::lineage_roots_route))
        .route(
            "/api/v1/lineage/has/{version_id}",
            get(routes::has_lineage_version_route),
        );

    // Provider health routes (always-on)
    let router = router
        .route(
            "/api/v1/providers/register",
            post(routes::register_provider_route),
        )
        .route(
            "/api/v1/providers/{name}/success",
            post(routes::record_provider_success_route),
        )
        .route(
            "/api/v1/providers/{name}/failure",
            post(routes::record_provider_failure_route),
        )
        .route(
            "/api/v1/providers/{name}/healthy",
            get(routes::is_healthy_route),
        )
        .route(
            "/api/v1/providers/health",
            get(routes::get_health_summary_route),
        );

    // Multi-provider routing (feature: multi-provider)
    #[cfg(feature = "multi-provider")]
    let router = router
        .route(
            "/api/v1/multi-provider/providers",
            post(routes::add_multi_provider_route),
        )
        .route(
            "/api/v1/multi-provider/route",
            get(routes::route_to_vendor_route),
        )
        .route(
            "/api/v1/multi-provider/providers/{name}/success",
            post(routes::record_multi_provider_success_route),
        )
        .route(
            "/api/v1/multi-provider/providers/{name}/failure",
            post(routes::record_multi_provider_failure_route),
        )
        .route(
            "/api/v1/multi-provider/stats",
            get(routes::provider_pool_stats_route),
        );

    // Gradual rollout (feature: gradual-rollout)
    #[cfg(feature = "gradual-rollout")]
    let router = router
        .route("/api/v1/rollouts/check", post(routes::check_rollout_route))
        .route("/api/v1/rollouts", post(routes::register_rollout_route))
        .route(
            "/api/v1/rollouts/inclusion",
            post(routes::find_rollout_inclusion_route),
        )
        .route(
            "/api/v1/rollouts/evaluate-rollback",
            post(routes::evaluate_auto_rollback_route),
        )
        .route(
            "/api/v1/rollouts/advance",
            post(routes::advance_segment_route),
        );

    // Safe deployment / rollback (feature: rollback)
    #[cfg(feature = "rollback")]
    let router = router
        .route("/api/v1/deploy", post(routes::deploy_with_rollback_route))
        .route(
            "/api/v1/rollback/{id}/restore",
            post(routes::restore_snapshot_route),
        )
        .route(
            "/api/v1/rollback/{id}/available",
            get(routes::is_rollback_available_route),
        );

    // Vibe coding — natural language → deliverable (feature: vibe)
    #[cfg(feature = "vibe")]
    let router = router.route("/api/v1/vibe/code", post(routes::vibe_code));

    // Cost estimation (feature: cost)
    #[cfg(feature = "cost")]
    let router = router.route("/api/v1/cost/estimate", post(routes::estimate_cost_route));

    // Feedback learning (feature: learn)
    #[cfg(feature = "learn")]
    let router = router.route("/api/v1/learn", post(routes::learn_from_feedback_route));

    // Confidence scoring (feature: confidence)
    #[cfg(feature = "confidence")]
    let router = router.route("/api/v1/confidence", post(routes::score_confidence_route));

    // Privacy scanning (feature: privacy)
    #[cfg(feature = "privacy")]
    let router = router.route("/api/v1/privacy/scan", post(routes::scan_privacy_route));

    // Fallback chain (feature: fallback)
    #[cfg(feature = "fallback")]
    let router = router.route("/api/v1/fallback", post(routes::fallback_chain_route));

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

    // Cost-limits tracking (feature: cost-limits)
    #[cfg(feature = "cost-limits")]
    let router = router
        .route(
            "/api/v1/cost-limits/check",
            post(routes::cost_limits_check_route),
        )
        .route(
            "/api/v1/cost-limits/limits",
            post(routes::cost_limits_set_limit_route),
        )
        .route(
            "/api/v1/cost-limits/utilization",
            get(routes::cost_limits_utilization_route),
        )
        .route(
            "/api/v1/cost-limits/status",
            get(routes::cost_limits_status_route),
        );

    // Beta-program management (feature: beta-program)
    #[cfg(feature = "beta-program")]
    let router = router
        .route(
            "/api/v1/beta/cohorts",
            post(routes::beta_create_cohort_route),
        )
        .route(
            "/api/v1/beta/cohorts/{cohort_id}/enroll",
            post(routes::beta_enroll_route),
        )
        .route(
            "/api/v1/beta/feedback",
            post(routes::beta_record_feedback_route),
        )
        .route("/api/v1/beta/stats", get(routes::beta_stats_route));

    // Token quota enforcement (feature: quota)
    #[cfg(feature = "quota")]
    let router = router
        .route("/api/v1/quota/consume", post(routes::quota_consume_route))
        .route("/api/v1/quota/usage", get(routes::quota_usage_route))
        .route("/api/v1/quota/reset", post(routes::quota_reset_route));

    // Content moderation (feature: moderation)
    #[cfg(feature = "moderation")]
    let router = router
        .route(
            "/api/v1/moderation/check",
            post(routes::moderation_check_route),
        )
        .route(
            "/api/v1/moderation/safe",
            post(routes::moderation_is_safe_route),
        )
        .route(
            "/api/v1/moderation/check-batch",
            post(routes::moderation_check_batch_route),
        );

    // Retention / GC routes (feature: retention)
    #[cfg(feature = "retention")]
    let router = router
        .route(
            "/api/v1/retention/period",
            post(routes::set_retention_period_route),
        )
        .route(
            "/api/v1/retention/period/{data_type}",
            get(routes::get_retention_period_route),
        )
        .route(
            "/api/v1/retention/expired",
            get(routes::is_data_expired_route),
        )
        .route(
            "/api/v1/retention/cleanup",
            post(routes::run_retention_cleanup_route),
        )
        .route("/api/v1/gc/run", post(routes::run_garbage_collection_route))
        .route(
            "/api/v1/gc/purge-soft-deleted",
            post(routes::purge_soft_deleted_route),
        )
        .route("/api/v1/gc/stats", get(routes::gc_stats_route))
        .route("/api/v1/gc/enabled", post(routes::set_gc_enabled_route))
        .route("/api/v1/gc/enabled", get(routes::gc_enabled_route));

    // Auto-purge routes (feature: auto-purge)
    #[cfg(feature = "auto-purge")]
    let router = router
        .route("/api/v1/auto-purge/purge", post(routes::purge_now_route))
        .route(
            "/api/v1/auto-purge/stats",
            get(routes::get_purge_stats_route),
        )
        .route(
            "/api/v1/auto-purge/config",
            post(routes::update_purge_config_route),
        )
        .route(
            "/api/v1/auto-purge/daemon/start",
            post(routes::start_purge_daemon_route),
        )
        .route(
            "/api/v1/auto-purge/daemon/stop",
            post(routes::stop_purge_daemon_route),
        );

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
