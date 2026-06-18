//! Multi-vendor model routing with health tracking and fallback chains.
//!
//! Allows PromptHub to route requests across multiple LLM providers,
//! automatically failover on health degradation.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use tracing::info;

/// Known LLM vendor identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Vendor {
    OpenAi,
    Anthropic,
    Google,
    Custom(String),
}

impl std::fmt::Display for Vendor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Vendor::OpenAi => write!(f, "openai"),
            Vendor::Anthropic => write!(f, "anthropic"),
            Vendor::Google => write!(f, "google"),
            Vendor::Custom(name) => write!(f, "{name}"),
        }
    }
}

/// A configured provider (vendor + endpoint).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub vendor: Vendor,
    pub endpoint: String,
    /// Priority for routing (lower = higher priority).
    pub priority: u32,
    /// Max retries before marking unhealthy.
    pub max_retries: u32,
}

impl ProviderConfig {
    pub fn new(name: &str, vendor: Vendor, endpoint: &str, priority: u32) -> Self {
        Self {
            name: name.to_string(),
            vendor,
            endpoint: endpoint.to_string(),
            priority,
            max_retries: 3,
        }
    }
}

/// Health status for a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,  // accepting some traffic
    Unhealthy, // no new traffic
}

impl HealthStatus {
    /// Check if this health status accepts new traffic.
    pub fn accepts_traffic(&self) -> bool {
        matches!(self, HealthStatus::Healthy | HealthStatus::Degraded)
    }
}

/// A tracked provider instance (config + live health state).
#[derive(Debug)]
pub struct TrackedProvider {
    pub config: ProviderConfig,
    pub health: HealthStatus,
    /// Consecutive failure count.
    pub failures: u32,
    /// Total successful requests (last 60s window).
    pub success_count: u64,
}

impl TrackedProvider {
    /// Create a new tracked provider from config with healthy status.
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            health: HealthStatus::Healthy,
            failures: 0,
            success_count: 0,
        }
    }

    /// Record a successful request for this provider.
    pub fn record_success(&mut self) {
        self.success_count += 1;
        if self.health == HealthStatus::Degraded && self.success_count.is_multiple_of(5) {
            info!(provider = %self.config.name, "Provider recovered to healthy");
            self.health = HealthStatus::Healthy;
        }
    }

    /// Record a failed request for this provider.
    pub fn record_failure(&mut self) {
        self.failures += 1;
        if self.failures >= self.config.max_retries && self.health != HealthStatus::Unhealthy {
            info!(provider = %self.config.name, failures = self.failures, "Provider marked unhealthy");
            self.health = HealthStatus::Degraded;
        }
        if self.failures >= self.config.max_retries * 2 {
            self.health = HealthStatus::Unhealthy;
        }
    }

    /// Check if this provider accepts new traffic.
    pub fn accepts_traffic(&self) -> bool {
        self.health.accepts_traffic()
    }

    /// Reset failure counts (e.g., after manual reset or timeout).
    pub fn reset_failures(&mut self) {
        self.failures = 0;
        self.health = HealthStatus::Healthy;
    }
}

/// A request routing decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// The selected provider for this request.
    pub provider_name: String,
    /// Which vendor is targeted.
    pub vendor: Vendor,
    /// Whether the selection was automatic (health-based) or manual override.
    pub strategy: RoutingStrategy,
}

/// How the routing decision was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingStrategy {
    /// Auto-selected from healthiest provider by priority.
    HealthBased,
    /// User explicitly requested this vendor.
    VendorOverride(Vendor),
    /// User explicitly requested this provider.
    ProviderOverride(String),
}

/// A multi-provider router managing all configured providers.
#[derive(Debug)]
pub struct MultiProviderRouter {
    providers: Vec<TrackedProvider>,
}

impl MultiProviderRouter {
    /// Create a new router with the given providers.
    pub fn new(providers: Vec<ProviderConfig>) -> Self {
        let tracked = providers.into_iter().map(TrackedProvider::new).collect();
        Self { providers: tracked }
    }

    /// Register a new provider to the routing pool.
    pub fn add_provider(&mut self, config: ProviderConfig) -> &TrackedProvider {
        self.providers.push(TrackedProvider::new(config.clone()));
        let len = self.providers.len();
        &self.providers[len - 1]
    }

    /// Get a provider by name.
    pub fn get_provider(&self, name: &str) -> Option<&TrackedProvider> {
        self.providers.iter().find(|p| p.config.name == name)
    }

    /// Select the best provider for routing based on health and priority.
    pub fn select(&self, vendor_filter: Option<Vendor>) -> Option<RoutingDecision> {
        let mut candidates: Vec<&TrackedProvider> = self
            .providers
            .iter()
            .filter(|p| p.accepts_traffic())
            .collect();

        if let Some(vend) = vendor_filter {
            candidates.retain(|p| p.config.vendor == vend);
        }

        if candidates.is_empty() {
            return None;
        }

        // Sort by priority (lower = higher), then by health.
        candidates.sort_by_key(|p| {
            let health_order = match p.health {
                HealthStatus::Healthy => 0,
                HealthStatus::Degraded => 1,
                HealthStatus::Unhealthy => 2,
            };
            (health_order, p.config.priority)
        });

        let best = candidates[0];
        Some(RoutingDecision {
            provider_name: best.config.name.clone(),
            vendor: best.config.vendor.clone(),
            strategy: RoutingStrategy::HealthBased,
        })
    }

    /// Record a successful request for a provider.
    pub fn record_success(&mut self, provider_name: &str) {
        if let Some(provider) = self
            .providers
            .iter_mut()
            .find(|p| p.config.name == provider_name)
        {
            provider.record_success();
        }
    }

    /// Record a failed request for a provider.
    pub fn record_failure(&mut self, provider_name: &str) {
        if let Some(provider) = self
            .providers
            .iter_mut()
            .find(|p| p.config.name == provider_name)
        {
            provider.record_failure();
        }
    }

    /// Reset health for a specific provider.
    pub fn reset_health(&mut self, provider_name: &str) -> bool {
        if let Some(provider) = self
            .providers
            .iter_mut()
            .find(|p| p.config.name == provider_name)
        {
            provider.reset_failures();
            true
        } else {
            false
        }
    }

    /// Get overall routing pool health stats.
    pub fn pool_stats(&self) -> PoolStats {
        let total = self.providers.len();
        let healthy = self
            .providers
            .iter()
            .filter(|p| p.health == HealthStatus::Healthy)
            .count();
        let degraded = self
            .providers
            .iter()
            .filter(|p| p.health == HealthStatus::Degraded)
            .count();
        let unhealthy = total - healthy - degraded;

        PoolStats {
            total_providers: total,
            healthy,
            degraded,
            unhealthy,
        }
    }

    /// Get all provider names currently accepting traffic.
    pub fn available_providers(&self) -> Vec<String> {
        self.providers
            .iter()
            .filter(|p| p.accepts_traffic())
            .map(|p| p.config.name.clone())
            .collect()
    }
}

/// Pool health statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub total_providers: usize,
    pub healthy: usize,
    pub degraded: usize,
    pub unhealthy: usize,
}

impl Default for MultiProviderRouter {
    fn default() -> Self {
        Self::new(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vendor_display() {
        assert_eq!(format!("{}", Vendor::OpenAi), "openai");
        assert_eq!(format!("{}", Vendor::Anthropic), "anthropic");
        assert_eq!(format!("{}", Vendor::Google), "google");
        assert_eq!(
            format!("{}", Vendor::Custom("my-vendor".to_string())),
            "my-vendor"
        );
    }

    #[test]
    fn test_health_accepts_traffic() {
        assert!(HealthStatus::Healthy.accepts_traffic());
        assert!(HealthStatus::Degraded.accepts_traffic());
        assert!(!HealthStatus::Unhealthy.accepts_traffic());
    }

    #[test]
    fn test_provider_record_success_recover() {
        let mut p = TrackedProvider::new(ProviderConfig::new(
            "test",
            Vendor::OpenAi,
            "https://test.com",
            1,
        ));
        // Fail three times to reach degraded threshold (max_retries defaults to 3).
        for _ in 0..3 {
            p.record_failure();
        }
        assert_eq!(p.health, HealthStatus::Degraded);

        // Record successes until recovery (every 5th triggers check).
        for _ in 0..10 {
            p.record_success();
        }
        assert_eq!(p.health, HealthStatus::Healthy);
    }

    #[test]
    fn test_provider_record_failure_unhealthy() {
        let mut p = TrackedProvider::new(ProviderConfig::new(
            "test",
            Vendor::OpenAi,
            "https://test.com",
            1,
        ));
        // max_retries defaults to 3.
        for _ in 0..3 {
            p.record_failure();
        }
        assert_eq!(p.health, HealthStatus::Degraded);

        // Double failures = unhealthy.
        for _ in 0..3 {
            p.record_failure();
        }
        assert_eq!(p.health, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_router_select_healthy() {
        let router = MultiProviderRouter::new(vec![
            ProviderConfig::new("openai", Vendor::OpenAi, "https://api.openai.com", 1),
            ProviderConfig::new(
                "anthropic",
                Vendor::Anthropic,
                "https://api.anthropic.com",
                2,
            ),
        ]);

        let decision = router.select(None).unwrap();
        assert_eq!(decision.provider_name, "openai");
        assert_eq!(decision.strategy, RoutingStrategy::HealthBased);
    }

    #[test]
    fn test_router_select_vendor_filter() {
        let router = MultiProviderRouter::new(vec![
            ProviderConfig::new("openai", Vendor::OpenAi, "https://api.openai.com", 1),
            ProviderConfig::new(
                "anthropic",
                Vendor::Anthropic,
                "https://api.anthropic.com",
                2,
            ),
        ]);

        let decision = router.select(Some(Vendor::Anthropic)).unwrap();
        assert_eq!(decision.provider_name, "anthropic");
    }

    #[test]
    fn test_router_select_unhealthy() {
        let mut router = MultiProviderRouter::new(vec![
            ProviderConfig::new("openai", Vendor::OpenAi, "https://api.openai.com", 1),
            ProviderConfig::new(
                "anthropic",
                Vendor::Anthropic,
                "https://api.anthropic.com",
                2,
            ),
        ]);

        // Mark openai as unhealthy.
        let p = router
            .providers
            .iter_mut()
            .find(|p| p.config.name == "openai")
            .unwrap();
        for _ in 0..6 {
            p.record_failure();
        }

        // Should route to the only healthy provider (anthropic).
        let decision = router.select(None).unwrap();
        assert_eq!(decision.provider_name, "anthropic");
    }

    #[test]
    fn test_router_add_provider() {
        let mut router = MultiProviderRouter::default();
        let new_config = ProviderConfig::new(
            "new-vendor",
            Vendor::Custom("custom".to_string()),
            "https://custom.com",
            5,
        );
        router.add_provider(new_config);
        assert_eq!(router.pool_stats().total_providers, 1);
    }

    #[test]
    fn test_router_pool_stats() {
        let router = MultiProviderRouter::new(vec![
            ProviderConfig::new("openai", Vendor::OpenAi, "https://api.openai.com", 1),
            ProviderConfig::new(
                "anthropic",
                Vendor::Anthropic,
                "https://api.anthropic.com",
                2,
            ),
        ]);

        let stats = router.pool_stats();
        assert_eq!(stats.healthy, 2);
        assert_eq!(stats.degraded, 0);
        assert_eq!(stats.unhealthy, 0);
    }

    #[test]
    fn test_router_available_providers() {
        let mut router = MultiProviderRouter::new(vec![ProviderConfig::new(
            "openai",
            Vendor::OpenAi,
            "https://api.openai.com",
            1,
        )]);

        let available = router.available_providers();
        assert_eq!(available, vec!["openai".to_string()]);

        // Mark unhealthy.
        router.record_failure("openai");
        router.record_failure("openai");
        for _ in 0..6 {
            router.record_failure("openai");
        }

        assert!(router.available_providers().is_empty());
    }

    #[test]
    fn test_router_reset_health() {
        let mut router = MultiProviderRouter::new(vec![ProviderConfig::new(
            "openai",
            Vendor::OpenAi,
            "https://api.openai.com",
            1,
        )]);

        for _ in 0..3 {
            router.record_failure("openai");
        }

        assert!(router.reset_health("openai"));
        assert_eq!(
            router.get_provider("openai").unwrap().health,
            HealthStatus::Healthy
        );
    }
}
