//! Multi-dimensional cost enforcement for prompt_hub.
//!
//! Extends the single-global-budget `BudgetTracker` with per-resource
//! limits, resource-type quotas, and configurable overage policies.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, instrument};

/// Resource type for which limits are tracked.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Resource {
    Compute,
    Storage,
    ApiCalls,
    Custom(String),
}

impl std::fmt::Display for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Resource::Compute => write!(f, "compute"),
            Resource::Storage => write!(f, "storage"),
            Resource::ApiCalls => write!(f, "api_calls"),
            Resource::Custom(name) => write!(f, "{name}"),
        }
    }
}

/// Policy applied when a limit is exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OveragePolicy {
    /// Allow spend but fire an alert (default).
    Alert,
    /// Block further spend in this bucket until the next period.
    Block,
    /// Hard-fail: returns an error immediately when exceeded.
    Fail,
}

/// A single limit entry for one entity-resource pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitEntry {
    pub entity_id: String,
    pub resource: Resource,
    pub budget_usd: f64,
    pub current_spend_micros: u64,
    pub overage_policy: OveragePolicy,
}

impl LimitEntry {
    /// Create a new limit entry.
    pub fn new(
        entity_id: &str,
        resource: Resource,
        budget_usd: f64,
        policy: OveragePolicy,
    ) -> Self {
        Self {
            entity_id: entity_id.to_string(),
            resource,
            budget_usd,
            current_spend_micros: 0,
            overage_policy: policy,
        }
    }

    /// Record spend against this limit entry.
    #[instrument(skip(self), fields(entity = %self.entity_id, resource = %self.resource, amount = amount_usd))]
    pub fn record(&mut self, amount_usd: f64) -> LimitStatus {
        let micros = (amount_usd * 1_000_000.0).round() as u64;
        self.current_spend_micros += micros;

        if self.is_exceeded() {
            match self.overage_policy {
                OveragePolicy::Block => LimitStatus::Blocked,
                OveragePolicy::Fail => LimitStatus::Failed(format!(
                    "Limit exceeded: ${:.2}/${:.2}",
                    self.current_spend_usd(),
                    self.budget_usd
                )),
                OveragePolicy::Alert => LimitStatus::OverLimit,
            }
        } else {
            info!(
                "Spend recorded: {} on resource={}",
                self.entity_id, self.resource
            );
            LimitStatus::Ok
        }
    }

    /// Check if the limit is exceeded.
    pub fn is_exceeded(&self) -> bool {
        let budget_micros = (self.budget_usd * 1_000_000.0).round() as u64;
        self.current_spend_micros >= budget_micros
    }

    /// Get current spend as USD.
    pub fn current_spend_usd(&self) -> f64 {
        self.current_spend_micros as f64 / 1_000_000.0
    }

    /// Get utilization as a percentage (0.0 to 100.0+).
    pub fn utilization_percent(&self) -> f64 {
        let budget_micros = (self.budget_usd * 1_000_000.0).round() as u64;
        if budget_micros == 0 {
            return 0.0;
        }
        (self.current_spend_micros as f64 / budget_micros as f64) * 100.0
    }

    /// Reset spend for a new billing period.
    pub fn reset(&mut self) {
        self.current_spend_micros = 0;
        info!(
            "Limit reset for {} on resource={}",
            self.entity_id, self.resource
        );
    }
}

/// Status returned after attempting to record spend against a limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitStatus {
    /// Spend was recorded successfully.
    Ok,
    /// Limit exceeded — overage policy is `Alert`.
    OverLimit,
    /// Limit exceeded — overage policy is `Block` (spend blocked).
    Blocked,
    /// Limit exceeded — overage policy is `Fail` (spend denied).
    Failed(String),
}

/// Config for the CostLimiter system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostLimitConfig {
    pub limits: HashMap<String, Vec<LimitEntry>>,
    pub default_overage_policy: OveragePolicy,
}

impl Default for CostLimitConfig {
    fn default() -> Self {
        Self {
            limits: HashMap::new(),
            default_overage_policy: OveragePolicy::Alert,
        }
    }
}

/// Multi-dimensional cost limiter.
#[derive(Debug)]
pub struct CostLimiter {
    config: Arc<std::sync::Mutex<CostLimitConfig>>,
}

impl CostLimiter {
    /// Create a new cost limiter with the given config.
    pub fn new(config: CostLimitConfig) -> Self {
        Self {
            config: Arc::new(std::sync::Mutex::new(config)),
        }
    }

    /// Get a copy of the underlying config.
    pub fn config(&self) -> CostLimitConfig {
        self.config.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// Add or update a limit for an entity-resource pair.
    #[instrument(skip(self))]
    pub fn set_limit(
        &self,
        entity_id: &str,
        resource: Resource,
        budget_usd: f64,
        policy: OveragePolicy,
    ) -> LimitEntry {
        let mut config = self.config.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let limit_resource = resource.clone();
        let limit = LimitEntry::new(entity_id, limit_resource, budget_usd, policy);

        if let Some(entries) = config.limits.get_mut(entity_id) {
            if let Some(entry) = entries.iter_mut().find(|e| e.resource == resource) {
                entry.budget_usd = budget_usd;
                entry.overage_policy = policy;
            } else {
                entries.push(limit.clone());
            }
        } else {
            config
                .limits
                .insert(entity_id.to_string(), vec![limit.clone()]);
        }

        limit
    }

    /// Check a limit and record spend in one atomic operation.
    ///
    /// Returns `LimitStatus::Blocked` or `OverLimit` if the limit is exceeded
    /// and enforcement would prevent further spend. Otherwise records the spend
    /// and returns `Ok`.
    #[instrument(skip(self))]
    pub fn check_and_record(
        &self,
        entity_id: &str,
        resource: Resource,
        amount_usd: f64,
    ) -> LimitStatus {
        let mut config = self.config.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

        let key = resource.clone();
        let micros = (amount_usd * 1_000_000.0).round() as u64;

        // Try to update existing entry — record FIRST, then check exceeded.
        if let Some(entry) = config
            .limits
            .get_mut(entity_id)
            .and_then(|e| e.iter_mut().find(|x| x.resource == key))
        {
            entry.current_spend_micros += micros;

            if entry.is_exceeded() {
                return match entry.overage_policy {
                    OveragePolicy::Alert => LimitStatus::OverLimit,
                    OveragePolicy::Block => LimitStatus::Blocked,
                    OveragePolicy::Fail => LimitStatus::Failed(format!(
                        "Limit exceeded: ${:.2}/${:.2}",
                        entry.current_spend_usd(),
                        entry.budget_usd,
                    )),
                };
            }

            info!(
                "Spend ${:.2} on entity={} resource={}",
                amount_usd, entity_id, key
            );
            return LimitStatus::Ok;
        }

        // No existing limit — create tracking entry with initial spend.
        if amount_usd > 0.0 {
            info!(
                "Spend ${:.2} on entity={} resource={}",
                amount_usd, entity_id, key
            );
        }
        let mut new_entry = LimitEntry::new(entity_id, key.clone(), f64::MAX, OveragePolicy::Alert);
        new_entry.current_spend_micros += micros;
        config
            .limits
            .entry(entity_id.to_string())
            .or_default()
            .push(new_entry);

        LimitStatus::Ok
    }

    /// Record spend against an entity's resource bucket (unconditional).
    pub fn record_unconditional(&self, entity_id: &str, resource: Resource, amount_usd: f64) {
        let key = resource.clone();
        if let Some(entry) = self
            .config
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .limits
            .get_mut(entity_id)
            .and_then(|e| e.iter_mut().find(|x| x.resource == key))
        {
            entry.current_spend_micros += (amount_usd * 1_000_000.0).round() as u64;
        }
    }

    /// Get utilization for a specific entity-resource pair.
    pub fn utilization(&self, entity_id: &str, resource: Resource) -> Option<f64> {
        let key = resource.clone();
        self.config
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .limits
            .get(entity_id)
            .and_then(|entries| entries.iter().find(|e| e.resource == key))
            .map(|e| e.utilization_percent())
    }

    /// Get all limit statuses for an entity.
    pub fn entity_status(&self, entity_id: &str) -> Vec<(Resource, f64, OveragePolicy)> {
        self.config
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .limits
            .get(entity_id)
            .map(|entries| {
                entries
                    .iter()
                    .map(|e| {
                        (
                            e.resource.clone(),
                            e.utilization_percent(),
                            e.overage_policy,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Reset all spend counters for a new billing period.
    pub fn reset_all(&self) {
        let mut config = self.config.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for entries in config.limits.values_mut() {
            for entry in entries.iter_mut() {
                entry.reset();
            }
        }
    }

    /// Get all entity IDs.
    pub fn entity_ids(&self) -> Vec<String> {
        self.config.lock().unwrap_or_else(std::sync::PoisonError::into_inner).limits.keys().cloned().collect()
    }
}

impl Default for CostLimiter {
    fn default() -> Self {
        Self::new(CostLimitConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limit_entry_new() {
        let entry = LimitEntry::new("org-1", Resource::Compute, 100.0, OveragePolicy::Alert);
        assert_eq!(entry.current_spend_usd(), 0.0);
        assert!(!entry.is_exceeded());
    }

    #[test]
    fn test_limit_entry_record_under() {
        let mut entry = LimitEntry::new("org-1", Resource::Compute, 100.0, OveragePolicy::Alert);
        let status = entry.record(50.0);
        assert_eq!(status, LimitStatus::Ok);
        assert_eq!(entry.current_spend_usd(), 50.0);
    }

    #[test]
    fn test_limit_entry_record_over() {
        let mut entry = LimitEntry::new("org-1", Resource::Compute, 100.0, OveragePolicy::Alert);
        entry.record(80.0);
        let status = entry.record(25.0);
        assert_eq!(status, LimitStatus::OverLimit);
    }

    #[test]
    fn test_limit_entry_block_policy() {
        let mut entry = LimitEntry::new("org-1", Resource::Compute, 100.0, OveragePolicy::Block);
        let status = entry.record(100.0);
        assert_eq!(status, LimitStatus::Blocked);
    }

    #[test]
    fn test_limit_entry_fail_policy() {
        let mut entry = LimitEntry::new("org-1", Resource::Compute, 100.0, OveragePolicy::Fail);
        let status = entry.record(105.0);
        assert!(matches!(status, LimitStatus::Failed(_)));
    }

    #[test]
    fn test_cost_limiter_set_limit() {
        let limiter = CostLimiter::default();
        limiter.set_limit("org-1", Resource::Compute, 500.0, OveragePolicy::Alert);

        let status = limiter.entity_status("org-1");
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].0, Resource::Compute);
    }

    #[test]
    fn test_cost_limiter_check_and_record_no_limit() {
        let limiter = CostLimiter::default();
        // No limits configured — should allow spend and create entry.
        let status = limiter.check_and_record("org-1", Resource::Compute, 10.0);
        assert_eq!(status, LimitStatus::Ok);
    }

    #[test]
    fn test_cost_limiter_check_and_record_with_limit() {
        let limiter = CostLimiter::default();
        limiter.set_limit("org-1", Resource::Compute, 100.0, OveragePolicy::Block);

        // Under limit — OK.
        let status = limiter.check_and_record("org-1", Resource::Compute, 50.0);
        assert_eq!(status, LimitStatus::Ok);

        // Over total limit (50+60=110>100) — blocked.
        let status = limiter.check_and_record("org-1", Resource::Compute, 60.0);
        assert_eq!(status, LimitStatus::Blocked);
    }

    #[test]
    fn test_cost_limiter_utilization() {
        let limiter = CostLimiter::default();
        limiter.set_limit("org-1", Resource::Storage, 200.0, OveragePolicy::Alert);

        let util = limiter.utilization("org-1", Resource::Storage).unwrap();
        assert_eq!(util, 0.0);
    }

    #[test]
    fn test_cost_limiter_entity_ids() {
        let limiter = CostLimiter::default();
        assert!(limiter.entity_ids().is_empty());

        limiter.set_limit("org-1", Resource::Compute, 100.0, OveragePolicy::Alert);
        assert_eq!(limiter.entity_ids(), vec!["org-1".to_string()]);
    }

    #[test]
    fn test_cost_limiter_reset_all() {
        let limiter = CostLimiter::default();
        limiter.set_limit("org-1", Resource::Compute, 100.0, OveragePolicy::Alert);
        // Simulate spend via direct access.
        let mut config = limiter.config.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(entries) = config.limits.get_mut("org-1") {
            entries[0].current_spend_micros = 90_000_000; // $90
        }
        drop(config);

        // Before reset — near limit.
        assert!(limiter.utilization("org-1", Resource::Compute).unwrap() > 80.0);

        limiter.reset_all();
        assert_eq!(
            limiter.utilization("org-1", Resource::Compute).unwrap(),
            0.0
        );
    }
}
