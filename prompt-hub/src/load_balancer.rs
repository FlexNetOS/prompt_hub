#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, instrument, warn};

/// LLM provider load balancer with multiple routing strategies.
///
/// Supports round-robin, weighted, and least-latency strategies
/// with health-aware request routing.
#[derive(Debug)]
pub struct LoadBalancer {
    providers: Vec<ProviderEntry>,
    strategy: RoutingStrategy,
    round_robin_idx: AtomicU64,
}

/// A provider entry with routing metadata.
///
/// Not `Clone`: it holds atomic counters for interior mutability behind
/// shared references, and atomics intentionally do not implement `Clone`.
#[derive(Debug)]
pub struct ProviderEntry {
    pub name: String,
    pub url: String,
    pub weight: u32,
    pub latency_ms: AtomicU64,
    pub healthy: std::sync::atomic::AtomicBool,
    pub request_count: AtomicU64,
    pub error_count: AtomicU64,
}

/// Routing strategy for selecting providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingStrategy {
    /// Distribute evenly across healthy providers
    RoundRobin,
    /// Route by configured weight
    Weighted,
    /// Route to the provider with lowest latency
    LeastLatency,
}

/// Provider selection result.
#[derive(Debug, Clone)]
pub struct ProviderSelection {
    pub provider_name: String,
    pub provider_url: String,
    pub strategy_used: RoutingStrategy,
}

impl LoadBalancer {
    /// Create a new load balancer with the given strategy.
    pub fn new(strategy: RoutingStrategy) -> Self {
        Self {
            providers: Vec::new(),
            strategy,
            round_robin_idx: AtomicU64::new(0),
        }
    }

    /// Add a provider to the pool.
    pub fn add_provider(&mut self, name: &str, url: &str, weight: u32) {
        self.providers.push(ProviderEntry {
            name: name.to_string(),
            url: url.to_string(),
            weight,
            latency_ms: AtomicU64::new(100),
            healthy: std::sync::atomic::AtomicBool::new(true),
            request_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
        });
        info!(
            "Added provider '{}' to load balancer (weight={})",
            name, weight
        );
    }

    /// Select a provider for the next request.
    #[instrument(skip(self))]
    pub fn select_provider(&self) -> Result<ProviderSelection> {
        let healthy: Vec<&ProviderEntry> = self
            .providers
            .iter()
            .filter(|p| p.healthy.load(Ordering::SeqCst))
            .collect();

        if healthy.is_empty() {
            warn!("No healthy providers available");
            return Err(HubError::Network(
                "No healthy LLM providers available".to_string(),
            ));
        }

        let selected = match self.strategy {
            RoutingStrategy::RoundRobin => self.round_robin(&healthy),
            RoutingStrategy::Weighted => self.weighted(&healthy),
            RoutingStrategy::LeastLatency => self.least_latency(&healthy),
        };

        if let Some(provider) = selected {
            provider.request_count.fetch_add(1, Ordering::SeqCst);
            Ok(ProviderSelection {
                provider_name: provider.name.clone(),
                provider_url: provider.url.clone(),
                strategy_used: self.strategy,
            })
        } else {
            Err(HubError::Network("Provider selection failed".to_string()))
        }
    }

    /// Mark a provider as healthy or unhealthy.
    pub fn set_health(&self, name: &str, healthy: bool) {
        for provider in &self.providers {
            if provider.name == name {
                provider.healthy.store(healthy, Ordering::SeqCst);
                if !healthy {
                    warn!("Provider '{}' marked as unhealthy", name);
                } else {
                    info!("Provider '{}' marked as healthy", name);
                }
            }
        }
    }

    /// Update latency for a provider.
    pub fn record_latency(&self, name: &str, latency_ms: u64) {
        for provider in &self.providers {
            if provider.name == name {
                provider.latency_ms.store(latency_ms, Ordering::SeqCst);
            }
        }
    }

    /// Record an error for a provider.
    pub fn record_error(&self, name: &str) {
        for provider in &self.providers {
            if provider.name == name {
                provider.error_count.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    /// Get pool statistics.
    pub fn stats(&self) -> Vec<ProviderStats> {
        self.providers
            .iter()
            .map(|p| ProviderStats {
                name: p.name.clone(),
                healthy: p.healthy.load(Ordering::SeqCst),
                latency_ms: p.latency_ms.load(Ordering::SeqCst),
                request_count: p.request_count.load(Ordering::SeqCst),
                error_count: p.error_count.load(Ordering::SeqCst),
            })
            .collect()
    }

    /// Number of registered providers.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Number of healthy providers.
    pub fn healthy_count(&self) -> usize {
        self.providers
            .iter()
            .filter(|p| p.healthy.load(Ordering::SeqCst))
            .count()
    }

    fn round_robin<'a>(&self, healthy: &[&'a ProviderEntry]) -> Option<&'a ProviderEntry> {
        let idx = self.round_robin_idx.fetch_add(1, Ordering::SeqCst) as usize;
        healthy.get(idx % healthy.len()).copied()
    }

    fn weighted<'a>(&self, healthy: &[&'a ProviderEntry]) -> Option<&'a ProviderEntry> {
        let total_weight: u32 = healthy.iter().map(|p| p.weight).sum();
        if total_weight == 0 {
            return healthy.first().copied();
        }
        // Simple weighted selection based on accumulated weights
        let idx = self.round_robin_idx.fetch_add(1, Ordering::SeqCst) as usize;
        let mut cursor = (idx as u32) % total_weight;
        for provider in healthy {
            if cursor < provider.weight {
                return Some(provider);
            }
            cursor -= provider.weight;
        }
        healthy.first().copied()
    }

    fn least_latency<'a>(&self, healthy: &[&'a ProviderEntry]) -> Option<&'a ProviderEntry> {
        healthy
            .iter()
            .min_by_key(|p| p.latency_ms.load(Ordering::SeqCst))
            .copied()
    }
}

/// Statistics for a single provider.
#[derive(Debug, Clone)]
pub struct ProviderStats {
    pub name: String,
    pub healthy: bool,
    pub latency_ms: u64,
    pub request_count: u64,
    pub error_count: u64,
}

impl Default for LoadBalancer {
    fn default() -> Self {
        Self::new(RoutingStrategy::RoundRobin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_provider() {
        let mut lb = LoadBalancer::new(RoutingStrategy::RoundRobin);
        lb.add_provider("openai", "https://api.openai.com", 10);
        assert_eq!(lb.provider_count(), 1);
    }

    #[test]
    fn test_round_robin_selection() {
        let mut lb = LoadBalancer::new(RoutingStrategy::RoundRobin);
        lb.add_provider("p1", "https://p1.com", 10);
        lb.add_provider("p2", "https://p2.com", 10);

        let sel1 = lb.select_provider().unwrap();
        let sel2 = lb.select_provider().unwrap();

        // Should alternate between providers
        assert_ne!(sel1.provider_name, sel2.provider_name);
    }

    #[test]
    fn test_weighted_selection() {
        let mut lb = LoadBalancer::new(RoutingStrategy::Weighted);
        lb.add_provider("heavy", "https://h.com", 10);
        lb.add_provider("light", "https://l.com", 1);

        let sel = lb.select_provider().unwrap();
        assert_eq!(sel.strategy_used, RoutingStrategy::Weighted);
    }

    #[test]
    fn test_least_latency_selection() {
        let mut lb = LoadBalancer::new(RoutingStrategy::LeastLatency);
        lb.add_provider("fast", "https://fast.com", 10);
        lb.add_provider("slow", "https://slow.com", 10);

        lb.record_latency("fast", 50);
        lb.record_latency("slow", 500);

        let sel = lb.select_provider().unwrap();
        assert_eq!(sel.provider_name, "fast");
    }

    #[test]
    fn test_no_healthy_providers() {
        let mut lb = LoadBalancer::new(RoutingStrategy::RoundRobin);
        lb.add_provider("p1", "https://p1.com", 10);
        lb.set_health("p1", false);

        assert!(lb.select_provider().is_err());
    }

    #[test]
    fn test_set_health() {
        let mut lb = LoadBalancer::new(RoutingStrategy::RoundRobin);
        lb.add_provider("p1", "https://p1.com", 10);
        assert_eq!(lb.healthy_count(), 1);

        lb.set_health("p1", false);
        assert_eq!(lb.healthy_count(), 0);

        lb.set_health("p1", true);
        assert_eq!(lb.healthy_count(), 1);
    }

    #[test]
    fn test_record_error() {
        let mut lb = LoadBalancer::new(RoutingStrategy::RoundRobin);
        lb.add_provider("p1", "https://p1.com", 10);
        lb.record_error("p1");

        let stats = lb.stats();
        assert_eq!(stats[0].error_count, 1);
    }

    #[test]
    fn test_stats() {
        let mut lb = LoadBalancer::new(RoutingStrategy::RoundRobin);
        lb.add_provider("p1", "https://p1.com", 10);
        lb.record_latency("p1", 200);

        let stats = lb.stats();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].latency_ms, 200);
        assert!(stats[0].healthy);
    }

    #[test]
    fn test_default() {
        let lb: LoadBalancer = Default::default();
        assert_eq!(lb.provider_count(), 0);
        assert_eq!(lb.healthy_count(), 0);
    }

    #[test]
    fn test_empty_pool_error() {
        let lb = LoadBalancer::new(RoutingStrategy::RoundRobin);
        assert!(lb.select_provider().is_err());
    }

    #[test]
    fn test_provider_selection_clone() {
        let sel = ProviderSelection {
            provider_name: "test".to_string(),
            provider_url: "https://test.com".to_string(),
            strategy_used: RoutingStrategy::RoundRobin,
        };
        let cloned = sel.clone();
        assert_eq!(cloned.provider_name, "test");
    }

    #[test]
    fn test_strategy_eq() {
        assert_eq!(RoutingStrategy::RoundRobin, RoutingStrategy::RoundRobin);
        assert_ne!(RoutingStrategy::RoundRobin, RoutingStrategy::Weighted);
    }
}
