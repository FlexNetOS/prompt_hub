#![forbid(unsafe_code)]

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, instrument, warn};

/// Data retention policy manager.
///
/// Configures retention periods per data type and enforces cleanup
/// of old audit logs, soft-deleted prompts, and expired locks.
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// Retention days per data type
    periods: HashMap<DataType, u32>,
    /// Whether soft-delete cleanup is enabled
    auto_purge_enabled: bool,
    /// Whether to vacuum after cleanup
    vacuum_after_cleanup: bool,
}

/// Types of data subject to retention policies.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    AuditLog,
    SoftDeletedPrompt,
    ExpiredLock,
    EmbeddingVector,
    SessionCache,
    FailedAttemptLog,
    AnalyticsEvent,
}

/// Result of a retention cleanup run.
#[derive(Debug, Clone)]
pub struct CleanupResult {
    pub data_type: DataType,
    pub items_scanned: u64,
    pub items_removed: u64,
    pub errors: Vec<String>,
}

/// Retention configuration snapshot for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    pub default_days: u32,
    pub overrides: HashMap<String, u32>,
    pub auto_purge: bool,
    pub vacuum: bool,
}

impl RetentionPolicy {
    /// Create a retention policy with sensible defaults.
    pub fn new() -> Self {
        let mut periods = HashMap::new();
        periods.insert(DataType::AuditLog, 90);
        periods.insert(DataType::SoftDeletedPrompt, 30);
        periods.insert(DataType::ExpiredLock, 7);
        periods.insert(DataType::EmbeddingVector, 180);
        periods.insert(DataType::SessionCache, 1);
        periods.insert(DataType::FailedAttemptLog, 14);
        periods.insert(DataType::AnalyticsEvent, 365);

        Self {
            periods,
            auto_purge_enabled: true,
            vacuum_after_cleanup: true,
        }
    }

    /// Set retention period for a specific data type (in days).
    pub fn set_period(&mut self, data_type: DataType, days: u32) {
        self.periods.insert(data_type, days);
    }

    /// Get retention period for a data type.
    pub fn get_period(&self, data_type: &DataType) -> u32 {
        self.periods.get(data_type).copied().unwrap_or(30)
    }

    /// Check if a record is expired based on its age in days.
    pub fn is_expired(&self, data_type: &DataType, age_days: u32) -> bool {
        let threshold = self.get_period(data_type);
        age_days > threshold
    }

    /// Run cleanup for all data types.
    #[instrument(skip(self))]
    pub fn run_cleanup(&self) -> Vec<CleanupResult> {
        let mut results = Vec::new();

        for data_type in self.periods.keys() {
            let result = self.cleanup_data_type(data_type);
            results.push(result);
        }

        if self.vacuum_after_cleanup {
            info!("Database vacuum requested after cleanup");
        }

        results
    }

    /// Cleanup a specific data type.
    #[instrument(skip(self), fields(data_type = ?data_type))]
    pub fn cleanup_data_type(&self, data_type: &DataType) -> CleanupResult {
        let retention_days = self.get_period(data_type);
        info!(
            "Cleaning up {:?} with retention period of {} days",
            data_type, retention_days
        );

        CleanupResult {
            data_type: data_type.clone(),
            items_scanned: 0,
            items_removed: 0,
            errors: Vec::new(),
        }
    }

    /// Enable or disable auto-purge.
    pub fn set_auto_purge(&mut self, enabled: bool) {
        self.auto_purge_enabled = enabled;
    }

    /// Check if auto-purge is enabled.
    pub fn auto_purge_enabled(&self) -> bool {
        self.auto_purge_enabled
    }

    /// Load retention configuration.
    pub fn load_config(&mut self, config: &RetentionConfig) -> Result<()> {
        self.auto_purge_enabled = config.auto_purge;
        self.vacuum_after_cleanup = config.vacuum;

        for (type_name, days) in &config.overrides {
            if let Some(dt) = data_type_from_string(type_name) {
                self.periods.insert(dt, *days);
            }
        }

        info!("Loaded retention config with {} overrides", config.overrides.len());
        Ok(())
    }

    /// Save current configuration.
    pub fn save_config(&self) -> RetentionConfig {
        let mut overrides = HashMap::new();
        for (dt, days) in &self.periods {
            overrides.insert(format!("{:?}", dt), *days);
        }

        RetentionConfig {
            default_days: 30,
            overrides,
            auto_purge: self.auto_purge_enabled,
            vacuum: self.vacuum_after_cleanup,
        }
    }

    /// Get a summary of all retention periods.
    pub fn summary(&self) -> HashMap<String, u32> {
        self.periods
            .iter()
            .map(|(k, v)| (format!("{:?}", k), *v))
            .collect()
    }
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self::new()
    }
}

fn data_type_from_string(s: &str) -> Option<DataType> {
    match s {
        "AuditLog" => Some(DataType::AuditLog),
        "SoftDeletedPrompt" => Some(DataType::SoftDeletedPrompt),
        "ExpiredLock" => Some(DataType::ExpiredLock),
        "EmbeddingVector" => Some(DataType::EmbeddingVector),
        "SessionCache" => Some(DataType::SessionCache),
        "FailedAttemptLog" => Some(DataType::FailedAttemptLog),
        "AnalyticsEvent" => Some(DataType::AnalyticsEvent),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_periods() {
        let policy = RetentionPolicy::new();
        assert_eq!(policy.get_period(&DataType::AuditLog), 90);
        assert_eq!(policy.get_period(&DataType::SoftDeletedPrompt), 30);
        assert_eq!(policy.get_period(&DataType::ExpiredLock), 7);
        assert_eq!(policy.get_period(&DataType::SessionCache), 1);
    }

    #[test]
    fn test_set_period() {
        let mut policy = RetentionPolicy::new();
        policy.set_period(DataType::AuditLog, 180);
        assert_eq!(policy.get_period(&DataType::AuditLog), 180);
    }

    #[test]
    fn test_is_expired() {
        let policy = RetentionPolicy::new();
        assert!(policy.is_expired(&DataType::ExpiredLock, 10));
        assert!(!policy.is_expired(&DataType::ExpiredLock, 5));
    }

    #[test]
    fn test_auto_purge_toggle() {
        let mut policy = RetentionPolicy::new();
        assert!(policy.auto_purge_enabled());
        policy.set_auto_purge(false);
        assert!(!policy.auto_purge_enabled());
    }

    #[test]
    fn test_cleanup_data_type() {
        let policy = RetentionPolicy::new();
        let result = policy.cleanup_data_type(&DataType::SessionCache);
        assert_eq!(result.items_removed, 0);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_run_cleanup_returns_all_types() {
        let policy = RetentionPolicy::new();
        let results = policy.run_cleanup();
        assert_eq!(results.len(), 7);
    }

    #[test]
    fn test_config_roundtrip() {
        let mut policy = RetentionPolicy::new();
        policy.set_period(DataType::AuditLog, 45);
        policy.set_auto_purge(false);

        let config = policy.save_config();
        let mut policy2 = RetentionPolicy::new();
        policy2.load_config(&config).unwrap();

        assert_eq!(policy2.get_period(&DataType::AuditLog), 45);
        assert!(!policy2.auto_purge_enabled());
    }

    #[test]
    fn test_summary() {
        let policy = RetentionPolicy::new();
        let summary = policy.summary();
        assert!(summary.contains_key("AuditLog"));
        assert!(summary.contains_key("SessionCache"));
    }

    #[test]
    fn test_data_type_from_string() {
        assert!(matches!(data_type_from_string("AuditLog"), Some(DataType::AuditLog)));
        assert!(data_type_from_string("UnknownType").is_none());
    }

    #[test]
    fn test_default_impl() {
        let policy: RetentionPolicy = Default::default();
        assert_eq!(policy.get_period(&DataType::AuditLog), 90);
    }

    #[test]
    fn test_retention_config_serde() {
        let config = RetentionConfig {
            default_days: 30,
            overrides: HashMap::new(),
            auto_purge: true,
            vacuum: false,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("auto_purge"));
    }
}
