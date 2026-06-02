#![forbid(unsafe_code)]

use crate::models::*;
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
        self.embedding_generation_ms.fetch_add(ms, Ordering::Relaxed);
        self.embedding_generation_count.fetch_add(1, Ordering::Relaxed);
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
        self.privacy_issues_found.fetch_add(issues_found, Ordering::Relaxed);
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
        if count > 0 {
            total / count
        } else {
            0
        }
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
        if count > 0 {
            total / count
        } else {
            0
        }
    }

    /// Get average DB query latency.
    pub fn get_avg_db_latency(&self) -> u64 {
        let total = self.db_query_latency_ms.load(Ordering::Relaxed);
        let count = self.db_query_latency_count.load(Ordering::Relaxed);
        if count > 0 {
            total / count
        } else {
            0
        }
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
}
