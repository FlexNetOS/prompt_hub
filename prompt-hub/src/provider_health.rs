#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::time::Instant;
use tracing::{info, instrument, warn};

/// Periodic health probe tracker for LLM providers.
///
/// Tracks latency, error rate, and availability for each provider
/// and updates their health status based on configurable thresholds.
#[derive(Debug, Clone)]
pub struct ProviderHealthMonitor {
    providers: Arc<RwLock<HashMap<String, ProviderHealthRecord>>>,
    latency_threshold_ms: u64,
    error_rate_threshold_percent: u8,
    probe_interval_secs: u64,
}

/// Health record for a single provider.
#[derive(Debug, Clone)]
pub struct ProviderHealthRecord {
    pub name: String,
    pub url: String,
    pub status: HealthStatus,
    pub last_probe: Option<Instant>,
    pub last_latency_ms: u64,
    pub error_count: u32,
    pub success_count: u32,
    pub consecutive_failures: u32,
    pub total_probes: u64,
}

/// Health status of a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Summary of all provider health statuses.
#[derive(Debug, Clone)]
pub struct HealthSummary {
    pub providers: Vec<ProviderHealthRecord>,
    pub healthy_count: usize,
    pub degraded_count: usize,
    pub unhealthy_count: usize,
    pub overall: HealthStatus,
}

impl ProviderHealthMonitor {
    /// Create a new health monitor.
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            latency_threshold_ms: 5000,
            error_rate_threshold_percent: 50,
            probe_interval_secs: 30,
        }
    }

    /// Register a provider to be monitored.
    #[instrument(skip(self))]
    pub fn register(&self, name: &str, url: &str) {
        let mut providers = self.providers.write().unwrap();
        providers.insert(
            name.to_string(),
            ProviderHealthRecord {
                name: name.to_string(),
                url: url.to_string(),
                status: HealthStatus::Unknown,
                last_probe: None,
                last_latency_ms: 0,
                error_count: 0,
                success_count: 0,
                consecutive_failures: 0,
                total_probes: 0,
            },
        );
        info!("Registered provider '{}' for health monitoring", name);
    }

    /// Record a successful probe result.
    #[instrument(skip(self), fields(name, latency_ms))]
    pub fn record_success(&self, name: &str, latency_ms: u64) {
        let mut providers = self.providers.write().unwrap();
        if let Some(record) = providers.get_mut(name) {
            record.last_probe = Some(Instant::now());
            record.last_latency_ms = latency_ms;
            record.success_count += 1;
            record.consecutive_failures = 0;
            record.total_probes += 1;

            // Update status based on latency
            record.status = if latency_ms > self.latency_threshold_ms {
                HealthStatus::Degraded
            } else {
                HealthStatus::Healthy
            };
        }
    }

    /// Record a failed probe result.
    #[instrument(skip(self), fields(name))]
    pub fn record_failure(&self, name: &str) {
        let mut providers = self.providers.write().unwrap();
        if let Some(record) = providers.get_mut(name) {
            record.last_probe = Some(Instant::now());
            record.error_count += 1;
            record.consecutive_failures += 1;
            record.total_probes += 1;

            let total = record.error_count + record.success_count;
            let error_rate = (record.error_count * 100).checked_div(total).unwrap_or(0);

            if record.consecutive_failures >= 3
                || error_rate as u8 >= self.error_rate_threshold_percent
            {
                record.status = HealthStatus::Unhealthy;
                warn!(
                    "Provider '{}' marked unhealthy (consecutive_failures={}, error_rate={}%)",
                    name, record.consecutive_failures, error_rate
                );
            } else {
                record.status = HealthStatus::Degraded;
            }
        }
    }

    /// Get health summary for all providers.
    pub fn summary(&self) -> HealthSummary {
        let providers = self.providers.read().unwrap();
        let records: Vec<ProviderHealthRecord> = providers.values().cloned().collect();

        let healthy = records
            .iter()
            .filter(|r| r.status == HealthStatus::Healthy)
            .count();
        let degraded = records
            .iter()
            .filter(|r| r.status == HealthStatus::Degraded)
            .count();
        let unhealthy = records
            .iter()
            .filter(|r| r.status == HealthStatus::Unhealthy)
            .count();

        let overall = if unhealthy > 0 || degraded > 0 {
            HealthStatus::Degraded
        } else if healthy > 0 {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unknown
        };

        HealthSummary {
            providers: records,
            healthy_count: healthy,
            degraded_count: degraded,
            unhealthy_count: unhealthy,
            overall,
        }
    }

    /// Get health for a specific provider.
    pub fn get_health(&self, name: &str) -> Option<HealthStatus> {
        let providers = self.providers.read().unwrap();
        providers.get(name).map(|r| r.status)
    }

    /// Check if a provider is healthy.
    pub fn is_healthy(&self, name: &str) -> bool {
        self.get_health(name)
            .map(|s| s == HealthStatus::Healthy)
            .unwrap_or(false)
    }

    /// Get number of registered providers.
    pub fn provider_count(&self) -> usize {
        self.providers.read().unwrap().len()
    }

    /// Configure thresholds.
    pub fn configure(&mut self, latency_ms: u64, error_rate_percent: u8) {
        self.latency_threshold_ms = latency_ms;
        self.error_rate_threshold_percent = error_rate_percent;
        info!(
            "Health monitor configured: latency_threshold={}ms, error_rate_threshold={}%",
            latency_ms, error_rate_percent
        );
    }

    /// Get the configured probe interval in seconds.
    pub fn probe_interval_secs(&self) -> u64 {
        self.probe_interval_secs
    }

    /// Remove a provider from monitoring.
    pub fn unregister(&self, name: &str) {
        let mut providers = self.providers.write().unwrap();
        providers.remove(name);
        info!("Unregistered provider '{}' from health monitoring", name);
    }
}

impl Default for ProviderHealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_provider() {
        let monitor = ProviderHealthMonitor::new();
        monitor.register("openai", "https://api.openai.com");
        assert_eq!(monitor.provider_count(), 1);
    }

    #[test]
    fn test_record_success() {
        let monitor = ProviderHealthMonitor::new();
        monitor.register("openai", "https://api.openai.com");
        monitor.record_success("openai", 200);
        assert!(monitor.is_healthy("openai"));
    }

    #[test]
    fn test_record_success_degraded() {
        let mut monitor = ProviderHealthMonitor::new();
        monitor.configure(100, 50); // low latency threshold
        monitor.register("openai", "https://api.openai.com");
        monitor.record_success("openai", 200); // exceeds 100ms threshold
        assert_eq!(monitor.get_health("openai"), Some(HealthStatus::Degraded));
    }

    #[test]
    fn test_record_failure() {
        let monitor = ProviderHealthMonitor::new();
        monitor.register("openai", "https://api.openai.com");
        monitor.record_failure("openai");
        let health = monitor.get_health("openai").unwrap();
        assert!(health == HealthStatus::Degraded || health == HealthStatus::Unhealthy);
    }

    #[test]
    fn test_consecutive_failures_make_unhealthy() {
        let monitor = ProviderHealthMonitor::new();
        monitor.register("openai", "https://api.openai.com");
        monitor.record_failure("openai");
        monitor.record_failure("openai");
        monitor.record_failure("openai");
        assert_eq!(monitor.get_health("openai"), Some(HealthStatus::Unhealthy));
    }

    #[test]
    fn test_summary() {
        let monitor = ProviderHealthMonitor::new();
        monitor.register("p1", "https://p1.com");
        monitor.register("p2", "https://p2.com");
        monitor.record_success("p1", 100);
        monitor.record_failure("p2");

        let summary = monitor.summary();
        assert_eq!(summary.providers.len(), 2);
        assert_eq!(summary.healthy_count, 1);
    }

    #[test]
    fn test_unregister() {
        let monitor = ProviderHealthMonitor::new();
        monitor.register("p1", "https://p1.com");
        assert_eq!(monitor.provider_count(), 1);
        monitor.unregister("p1");
        assert_eq!(monitor.provider_count(), 0);
    }

    #[test]
    fn test_is_healthy_unknown() {
        let monitor = ProviderHealthMonitor::new();
        assert!(!monitor.is_healthy("nonexistent"));
    }

    #[test]
    fn test_default() {
        let monitor: ProviderHealthMonitor = Default::default();
        assert_eq!(monitor.provider_count(), 0);
    }

    #[test]
    fn test_configure() {
        let mut monitor = ProviderHealthMonitor::new();
        monitor.configure(3000, 25);
        monitor.register("p1", "https://p1.com");
        monitor.record_success("p1", 4000);
        assert_eq!(monitor.get_health("p1"), Some(HealthStatus::Degraded));
    }

    #[test]
    fn test_health_status_eq() {
        assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
        assert_ne!(HealthStatus::Healthy, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_recovery_after_failure() {
        let monitor = ProviderHealthMonitor::new();
        monitor.register("p1", "https://p1.com");
        // Make unhealthy
        monitor.record_failure("p1");
        monitor.record_failure("p1");
        monitor.record_failure("p1");
        assert_eq!(monitor.get_health("p1"), Some(HealthStatus::Unhealthy));
        // Recover
        monitor.record_success("p1", 50);
        assert_eq!(monitor.get_health("p1"), Some(HealthStatus::Healthy));
    }

    #[test]
    fn test_health_summary_overall() {
        let monitor = ProviderHealthMonitor::new();
        monitor.register("p1", "https://p1.com");
        monitor.register("p2", "https://p2.com");
        monitor.record_failure("p1");
        monitor.record_failure("p1");
        monitor.record_failure("p1");
        monitor.record_success("p2", 100);
        let summary = monitor.summary();
        assert_eq!(summary.overall, HealthStatus::Degraded);
    }
}
