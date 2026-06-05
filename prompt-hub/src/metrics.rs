#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

/// Metrics collector for observability.
///
/// Tracks request counts, latencies, active locks, and feature-specific
/// counters using atomic operations for thread safety.
#[derive(Debug)]
pub struct MetricsCollector {
    requests_total: AtomicU64,
    search_latency_ms: AtomicU64,
    search_latency_count: AtomicU64,
    embedding_generation_ms: AtomicU64,
    embedding_generation_count: AtomicU64,
    db_query_latency_ms: AtomicU64,
    db_query_latency_count: AtomicU64,
    active_locks: AtomicU64,
    sanitization_blocked: AtomicU64,
    evolution_success: AtomicU64,
    evolution_failure: AtomicU64,
    pollination_patterns: AtomicU64,
    privacy_scans: AtomicU64,
    privacy_issues_found: AtomicU64,
    quality_gate_runs: AtomicU64,
    quality_gate_failures: AtomicU64,
    multimodal_processed: AtomicU64,
    rollback_deployments: AtomicU64,
    rollback_rollbacked: AtomicU64,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self {
            requests_total: AtomicU64::new(0),
            search_latency_ms: AtomicU64::new(0),
            search_latency_count: AtomicU64::new(0),
            embedding_generation_ms: AtomicU64::new(0),
            embedding_generation_count: AtomicU64::new(0),
            db_query_latency_ms: AtomicU64::new(0),
            db_query_latency_count: AtomicU64::new(0),
            active_locks: AtomicU64::new(0),
            sanitization_blocked: AtomicU64::new(0),
            evolution_success: AtomicU64::new(0),
            evolution_failure: AtomicU64::new(0),
            pollination_patterns: AtomicU64::new(0),
            privacy_scans: AtomicU64::new(0),
            privacy_issues_found: AtomicU64::new(0),
            quality_gate_runs: AtomicU64::new(0),
            quality_gate_failures: AtomicU64::new(0),
            multimodal_processed: AtomicU64::new(0),
            rollback_deployments: AtomicU64::new(0),
            rollback_rollbacked: AtomicU64::new(0),
        }
    }
}

impl MetricsCollector {
    /// Create a new metrics collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a request.
    pub fn record_request(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record search latency in milliseconds.
    pub fn record_search_latency(&self, ms: u64) {
        self.search_latency_ms.fetch_add(ms, Ordering::Relaxed);
        self.search_latency_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record embedding generation latency in milliseconds.
    pub fn record_embedding_generation(&self, ms: u64) {
        self.embedding_generation_ms
            .fetch_add(ms, Ordering::Relaxed);
        self.embedding_generation_count
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record database query latency in milliseconds.
    pub fn record_db_query_latency(&self, ms: u64) {
        self.db_query_latency_ms.fetch_add(ms, Ordering::Relaxed);
        self.db_query_latency_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a lock acquisition.
    pub fn record_lock_acquired(&self) {
        self.active_locks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a lock release.
    pub fn record_lock_released(&self) {
        self.active_locks.fetch_sub(1, Ordering::Relaxed);
    }

    /// Record a blocked sanitization attempt.
    pub fn record_sanitization_blocked(&self) {
        self.sanitization_blocked.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a successful evolution.
    pub fn record_evolution_success(&self) {
        self.evolution_success.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a failed evolution.
    pub fn record_evolution_failure(&self) {
        self.evolution_failure.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a shared pattern in pollination.
    pub fn record_pollination_pattern(&self) {
        self.pollination_patterns.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a completed privacy scan.
    pub fn record_privacy_scan(&self, issues_found: u64) {
        self.privacy_scans.fetch_add(1, Ordering::Relaxed);
        self.privacy_issues_found
            .fetch_add(issues_found, Ordering::Relaxed);
    }

    /// Record a quality gate run.
    pub fn record_quality_gate(&self, passed: bool) {
        self.quality_gate_runs.fetch_add(1, Ordering::Relaxed);
        if !passed {
            self.quality_gate_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a processed multimodal input.
    pub fn record_multimodal_processed(&self) {
        self.multimodal_processed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a deployment attempt.
    pub fn record_deployment(&self) {
        self.rollback_deployments.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a rollback event.
    pub fn record_rollback(&self) {
        self.rollback_rollbacked.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total number of requests.
    pub fn get_requests_total(&self) -> u64 {
        self.requests_total.load(Ordering::Relaxed)
    }

    /// Get current number of active locks.
    pub fn get_active_locks(&self) -> u64 {
        self.active_locks.load(Ordering::Relaxed)
    }

    /// Get average search latency, or 0 if no searches recorded.
    pub fn get_avg_search_latency(&self) -> u64 {
        let total = self.search_latency_ms.load(Ordering::Relaxed);
        let count = self.search_latency_count.load(Ordering::Relaxed);
        total.checked_div(count).unwrap_or(0)
    }

    /// Get total number of blocked sanitization attempts.
    pub fn get_sanitization_blocked(&self) -> u64 {
        self.sanitization_blocked.load(Ordering::Relaxed)
    }

    /// Get total successful evolutions.
    pub fn get_evolution_success(&self) -> u64 {
        self.evolution_success.load(Ordering::Relaxed)
    }

    /// Get total failed evolutions.
    pub fn get_evolution_failure(&self) -> u64 {
        self.evolution_failure.load(Ordering::Relaxed)
    }

    /// Get total shared pollination patterns.
    pub fn get_pollination_patterns(&self) -> u64 {
        self.pollination_patterns.load(Ordering::Relaxed)
    }

    /// Get total privacy scans completed.
    pub fn get_privacy_scans(&self) -> u64 {
        self.privacy_scans.load(Ordering::Relaxed)
    }

    /// Get total privacy issues found.
    pub fn get_privacy_issues_found(&self) -> u64 {
        self.privacy_issues_found.load(Ordering::Relaxed)
    }

    /// Get total quality gate runs.
    pub fn get_quality_gate_runs(&self) -> u64 {
        self.quality_gate_runs.load(Ordering::Relaxed)
    }

    /// Get total quality gate failures.
    pub fn get_quality_gate_failures(&self) -> u64 {
        self.quality_gate_failures.load(Ordering::Relaxed)
    }

    /// Get total multimodal inputs processed.
    pub fn get_multimodal_processed(&self) -> u64 {
        self.multimodal_processed.load(Ordering::Relaxed)
    }

    /// Get total deployments attempted.
    pub fn get_deployments(&self) -> u64 {
        self.rollback_deployments.load(Ordering::Relaxed)
    }

    /// Get total rollbacks performed.
    pub fn get_rollbacks(&self) -> u64 {
        self.rollback_rollbacked.load(Ordering::Relaxed)
    }

    /// Get average embedding generation latency.
    pub fn get_avg_embedding_latency(&self) -> u64 {
        let total = self.embedding_generation_ms.load(Ordering::Relaxed);
        let count = self.embedding_generation_count.load(Ordering::Relaxed);
        total.checked_div(count).unwrap_or(0)
    }

    /// Get average DB query latency.
    pub fn get_avg_db_latency(&self) -> u64 {
        let total = self.db_query_latency_ms.load(Ordering::Relaxed);
        let count = self.db_query_latency_count.load(Ordering::Relaxed);
        total.checked_div(count).unwrap_or(0)
    }

    // Raw cumulative sum/count accessors for the latency aggregates. These back
    // the Prometheus exposition's `*_sum` / `*_count` series, from which a
    // scrape can derive a rate-based average — the correct representation for a
    // sum+count aggregate (as opposed to a single-bucket pseudo-histogram).

    /// Get the cumulative sum of recorded search latencies (milliseconds).
    pub fn get_search_latency_sum(&self) -> u64 {
        self.search_latency_ms.load(Ordering::Relaxed)
    }

    /// Get the number of recorded search-latency samples.
    pub fn get_search_latency_count(&self) -> u64 {
        self.search_latency_count.load(Ordering::Relaxed)
    }

    /// Get the cumulative sum of recorded embedding-generation latencies (milliseconds).
    pub fn get_embedding_latency_sum(&self) -> u64 {
        self.embedding_generation_ms.load(Ordering::Relaxed)
    }

    /// Get the number of recorded embedding-generation latency samples.
    pub fn get_embedding_latency_count(&self) -> u64 {
        self.embedding_generation_count.load(Ordering::Relaxed)
    }

    /// Get the cumulative sum of recorded DB-query latencies (milliseconds).
    pub fn get_db_latency_sum(&self) -> u64 {
        self.db_query_latency_ms.load(Ordering::Relaxed)
    }

    /// Get the number of recorded DB-query latency samples.
    pub fn get_db_latency_count(&self) -> u64 {
        self.db_query_latency_count.load(Ordering::Relaxed)
    }

    /// Report all metrics as a formatted summary string.
    pub fn summary(&self) -> String {
        format!(
            "Metrics Summary:\n  Requests: {}\n  Avg Search Latency: {}ms\n  Avg Embedding Latency: {}ms\n  Avg DB Latency: {}ms\n  Active Locks: {}\n  Sanitization Blocked: {}\n  Evolution Success: {}\n  Evolution Failure: {}\n  Pollination Patterns: {}\n  Privacy Scans: {}\n  Quality Gate Runs: {}\n  Quality Gate Failures: {}\n  Multimodal Processed: {}\n  Deployments: {}\n  Rollbacks: {}",
            self.get_requests_total(),
            self.get_avg_search_latency(),
            self.get_avg_embedding_latency(),
            self.get_avg_db_latency(),
            self.get_active_locks(),
            self.get_sanitization_blocked(),
            self.get_evolution_success(),
            self.get_evolution_failure(),
            self.get_pollination_patterns(),
            self.get_privacy_scans(),
            self.get_quality_gate_runs(),
            self.get_quality_gate_failures(),
            self.get_multimodal_processed(),
            self.get_deployments(),
            self.get_rollbacks(),
        )
    }

    /// Reset all counters to zero.
    pub fn reset(&self) {
        self.requests_total.store(0, Ordering::Relaxed);
        self.search_latency_ms.store(0, Ordering::Relaxed);
        self.search_latency_count.store(0, Ordering::Relaxed);
        self.embedding_generation_ms.store(0, Ordering::Relaxed);
        self.embedding_generation_count.store(0, Ordering::Relaxed);
        self.db_query_latency_ms.store(0, Ordering::Relaxed);
        self.db_query_latency_count.store(0, Ordering::Relaxed);
        self.active_locks.store(0, Ordering::Relaxed);
        self.sanitization_blocked.store(0, Ordering::Relaxed);
        self.evolution_success.store(0, Ordering::Relaxed);
        self.evolution_failure.store(0, Ordering::Relaxed);
        self.pollination_patterns.store(0, Ordering::Relaxed);
        self.privacy_scans.store(0, Ordering::Relaxed);
        self.privacy_issues_found.store(0, Ordering::Relaxed);
        self.quality_gate_runs.store(0, Ordering::Relaxed);
        self.quality_gate_failures.store(0, Ordering::Relaxed);
        self.multimodal_processed.store(0, Ordering::Relaxed);
        self.rollback_deployments.store(0, Ordering::Relaxed);
        self.rollback_rollbacked.store(0, Ordering::Relaxed);
        info!("All metrics reset to zero");
    }

    /// Render all metrics in the Prometheus text exposition format (v0.0.4).
    ///
    /// Builds a fresh [`prometheus::Registry`] from the current atomic snapshot
    /// on each call and encodes it with the text encoder — no protobuf, no
    /// process-global meter provider. Counters carry the `_total` suffix;
    /// `active_locks` is a gauge. Latency aggregates are exposed as cumulative
    /// `*_ms_sum` / `*_ms_count` counter pairs, the correct representation for a
    /// precomputed sum+count (a scrape derives the average via
    /// `rate(sum) / rate(count)`) — deliberately *not* a single-bucket
    /// pseudo-histogram.
    #[cfg(feature = "otel")]
    pub fn prometheus_text(&self) -> crate::Result<String> {
        use prometheus::{Encoder, IntCounter, IntGauge, Opts, Registry, TextEncoder};

        let registry = Registry::new();

        let counter = |name: &str, help: &str, value: u64| -> crate::Result<()> {
            let c = IntCounter::with_opts(Opts::new(name, help)).map_err(|e| {
                crate::HubError::Internal(format!("prometheus counter {name}: {e}"))
            })?;
            if value > 0 {
                c.inc_by(value);
            }
            registry
                .register(Box::new(c))
                .map_err(|e| crate::HubError::Internal(format!("prometheus register {name}: {e}")))
        };

        let gauge = |name: &str, help: &str, value: i64| -> crate::Result<()> {
            let g = IntGauge::with_opts(Opts::new(name, help))
                .map_err(|e| crate::HubError::Internal(format!("prometheus gauge {name}: {e}")))?;
            g.set(value);
            registry
                .register(Box::new(g))
                .map_err(|e| crate::HubError::Internal(format!("prometheus register {name}: {e}")))
        };

        counter(
            "prompt_hub_requests_total",
            "Total requests processed",
            self.get_requests_total(),
        )?;
        counter(
            "prompt_hub_sanitization_blocked_total",
            "Sanitization attempts blocked",
            self.get_sanitization_blocked(),
        )?;
        counter(
            "prompt_hub_evolution_success_total",
            "Successful prompt evolutions",
            self.get_evolution_success(),
        )?;
        counter(
            "prompt_hub_evolution_failure_total",
            "Failed prompt evolutions",
            self.get_evolution_failure(),
        )?;
        counter(
            "prompt_hub_pollination_patterns_total",
            "Patterns shared via pollination",
            self.get_pollination_patterns(),
        )?;
        counter(
            "prompt_hub_privacy_scans_total",
            "Privacy scans completed",
            self.get_privacy_scans(),
        )?;
        counter(
            "prompt_hub_privacy_issues_found_total",
            "Privacy issues found across all scans",
            self.get_privacy_issues_found(),
        )?;
        counter(
            "prompt_hub_quality_gate_runs_total",
            "Quality gate runs",
            self.get_quality_gate_runs(),
        )?;
        counter(
            "prompt_hub_quality_gate_failures_total",
            "Quality gate failures",
            self.get_quality_gate_failures(),
        )?;
        counter(
            "prompt_hub_multimodal_processed_total",
            "Multimodal inputs processed",
            self.get_multimodal_processed(),
        )?;
        counter(
            "prompt_hub_deployments_total",
            "Deployments attempted",
            self.get_deployments(),
        )?;
        counter(
            "prompt_hub_rollbacks_total",
            "Rollbacks performed",
            self.get_rollbacks(),
        )?;

        // Latency aggregates as sum/count counter pairs (milliseconds).
        counter(
            "prompt_hub_search_latency_ms_sum",
            "Cumulative search latency in milliseconds",
            self.get_search_latency_sum(),
        )?;
        counter(
            "prompt_hub_search_latency_ms_count",
            "Number of search-latency samples",
            self.get_search_latency_count(),
        )?;
        counter(
            "prompt_hub_embedding_latency_ms_sum",
            "Cumulative embedding-generation latency in milliseconds",
            self.get_embedding_latency_sum(),
        )?;
        counter(
            "prompt_hub_embedding_latency_ms_count",
            "Number of embedding-generation latency samples",
            self.get_embedding_latency_count(),
        )?;
        counter(
            "prompt_hub_db_latency_ms_sum",
            "Cumulative DB-query latency in milliseconds",
            self.get_db_latency_sum(),
        )?;
        counter(
            "prompt_hub_db_latency_ms_count",
            "Number of DB-query latency samples",
            self.get_db_latency_count(),
        )?;

        // Gauges.
        gauge(
            "prompt_hub_active_locks",
            "Currently held locks",
            self.get_active_locks() as i64,
        )?;

        let mut buf = Vec::new();
        TextEncoder::new()
            .encode(&registry.gather(), &mut buf)
            .map_err(|e| crate::HubError::Internal(format!("prometheus encode: {e}")))?;
        String::from_utf8(buf)
            .map_err(|e| crate::HubError::Internal(format!("prometheus utf8: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_request() {
        let metrics = MetricsCollector::new();
        assert_eq!(metrics.get_requests_total(), 0);
        metrics.record_request();
        assert_eq!(metrics.get_requests_total(), 1);
        metrics.record_request();
        assert_eq!(metrics.get_requests_total(), 2);
    }

    #[test]
    fn test_lock_tracking() {
        let metrics = MetricsCollector::new();
        assert_eq!(metrics.get_active_locks(), 0);
        metrics.record_lock_acquired();
        assert_eq!(metrics.get_active_locks(), 1);
        metrics.record_lock_acquired();
        assert_eq!(metrics.get_active_locks(), 2);
        metrics.record_lock_released();
        assert_eq!(metrics.get_active_locks(), 1);
    }

    #[test]
    fn test_search_latency_average() {
        let metrics = MetricsCollector::new();
        assert_eq!(metrics.get_avg_search_latency(), 0);
        metrics.record_search_latency(100);
        metrics.record_search_latency(200);
        assert_eq!(metrics.get_avg_search_latency(), 150);
    }

    #[test]
    fn test_embedding_latency_average() {
        let metrics = MetricsCollector::new();
        assert_eq!(metrics.get_avg_embedding_latency(), 0);
        metrics.record_embedding_generation(50);
        metrics.record_embedding_generation(150);
        assert_eq!(metrics.get_avg_embedding_latency(), 100);
    }

    #[test]
    fn test_db_latency_average() {
        let metrics = MetricsCollector::new();
        assert_eq!(metrics.get_avg_db_latency(), 0);
        metrics.record_db_query_latency(10);
        metrics.record_db_query_latency(30);
        assert_eq!(metrics.get_avg_db_latency(), 20);
    }

    #[test]
    fn test_evolution_counters() {
        let metrics = MetricsCollector::new();
        assert_eq!(metrics.get_evolution_success(), 0);
        assert_eq!(metrics.get_evolution_failure(), 0);
        metrics.record_evolution_success();
        metrics.record_evolution_success();
        metrics.record_evolution_failure();
        assert_eq!(metrics.get_evolution_success(), 2);
        assert_eq!(metrics.get_evolution_failure(), 1);
    }

    #[test]
    fn test_privacy_counters() {
        let metrics = MetricsCollector::new();
        metrics.record_privacy_scan(5);
        metrics.record_privacy_scan(3);
        assert_eq!(metrics.get_privacy_scans(), 2);
        assert_eq!(metrics.get_privacy_issues_found(), 8);
    }

    #[test]
    fn test_quality_gate_counters() {
        let metrics = MetricsCollector::new();
        metrics.record_quality_gate(true);
        metrics.record_quality_gate(false);
        metrics.record_quality_gate(true);
        assert_eq!(metrics.get_quality_gate_runs(), 3);
        assert_eq!(metrics.get_quality_gate_failures(), 1);
    }

    #[test]
    fn test_rollback_counters() {
        let metrics = MetricsCollector::new();
        metrics.record_deployment();
        metrics.record_deployment();
        metrics.record_rollback();
        assert_eq!(metrics.get_deployments(), 2);
        assert_eq!(metrics.get_rollbacks(), 1);
    }

    #[test]
    fn test_multimodal_counter() {
        let metrics = MetricsCollector::new();
        assert_eq!(metrics.get_multimodal_processed(), 0);
        metrics.record_multimodal_processed();
        assert_eq!(metrics.get_multimodal_processed(), 1);
    }

    #[test]
    fn test_sanitization_counter() {
        let metrics = MetricsCollector::new();
        assert_eq!(metrics.get_sanitization_blocked(), 0);
        metrics.record_sanitization_blocked();
        assert_eq!(metrics.get_sanitization_blocked(), 1);
    }

    #[test]
    fn test_pollination_counter() {
        let metrics = MetricsCollector::new();
        assert_eq!(metrics.get_pollination_patterns(), 0);
        metrics.record_pollination_pattern();
        assert_eq!(metrics.get_pollination_patterns(), 1);
    }

    #[test]
    fn test_summary() {
        let metrics = MetricsCollector::new();
        metrics.record_request();
        metrics.record_request();
        metrics.record_search_latency(100);
        metrics.record_lock_acquired();

        let summary = metrics.summary();
        assert!(summary.contains("Requests: 2"));
        assert!(summary.contains("Active Locks: 1"));
    }

    #[test]
    fn test_reset() {
        let metrics = MetricsCollector::new();
        metrics.record_request();
        metrics.record_request();
        metrics.record_lock_acquired();
        metrics.record_evolution_success();

        metrics.reset();

        assert_eq!(metrics.get_requests_total(), 0);
        assert_eq!(metrics.get_active_locks(), 0);
        assert_eq!(metrics.get_evolution_success(), 0);
        assert_eq!(metrics.get_avg_search_latency(), 0);
    }

    #[test]
    fn test_latency_sum_count_accessors() {
        let metrics = MetricsCollector::new();
        metrics.record_search_latency(100);
        metrics.record_search_latency(50);
        assert_eq!(metrics.get_search_latency_sum(), 150);
        assert_eq!(metrics.get_search_latency_count(), 2);
        // avg is derived from the same sum/count
        assert_eq!(metrics.get_avg_search_latency(), 75);
    }

    #[cfg(feature = "otel")]
    #[test]
    fn test_prometheus_text_is_valid_exposition() {
        let metrics = MetricsCollector::new();
        metrics.record_request();
        metrics.record_request();
        metrics.record_search_latency(100);
        metrics.record_lock_acquired();
        metrics.record_sanitization_blocked();

        let text = metrics.prometheus_text().expect("exposition renders");

        // Correctly typed series.
        assert!(text.contains("# TYPE prompt_hub_requests_total counter"));
        assert!(text.contains("prompt_hub_requests_total 2"));
        assert!(text.contains("# TYPE prompt_hub_active_locks gauge"));
        assert!(text.contains("prompt_hub_active_locks 1"));
        assert!(text.contains("prompt_hub_sanitization_blocked_total 1"));

        // Latency exposed as sum/count, not a malformed single-bucket histogram.
        assert!(text.contains("prompt_hub_search_latency_ms_sum 100"));
        assert!(text.contains("prompt_hub_search_latency_ms_count 1"));
        assert!(
            !text.contains("le=\"+Inf\""),
            "must not emit a single-bucket pseudo-histogram: {text}"
        );
        assert!(
            !text.contains("histogram"),
            "no histogram-typed series without real buckets: {text}"
        );
    }
}
