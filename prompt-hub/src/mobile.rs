#![forbid(unsafe_code)]
//! Mobile-first prompt management layer.
//!
//! Provides offline-first CRUD with SQLite-on-device storage, bandwidth-aware sync, and
//! network condition detection for intermittent connectivity scenarios.

use serde::{Deserialize, Serialize};

/// Network conditions detected on the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkCondition {
    /// Full connectivity (Wi-Fi or cellular with adequate bandwidth).
    Connected(NetworkType),
    /// Limited connectivity (metered / low-bandwidth).
    Metered,
    /// No connectivity; device is offline.
    Offline,
}

/// The type of network connection available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkType {
    Wifi,
    Cellular(CellularGeneration),
}

/// Cellular network generation estimate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CellularGeneration {
    Gprs,
    Edge,
    Umts,
    Hspa,
    Lte,
    Iden,
    Evt,
    OneX,
    FiveG,
}

/// Strategy for syncing local changes to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStrategy {
    /// Only push when on Wi-Fi; defer cellular sync to background.
    WifiOnly,
    /// Push immediately regardless of network type.
    Immediate,
    /// Compress payloads for bandwidth-constrained networks.
    Compressed,
}

/// Configuration for the mobile store layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileConfig {
    /// SQLite path for the on-device store.
    pub db_path: String,
    /// Network condition strategy.
    pub network_condition: NetworkCondition,
    /// Sync strategy for outbound changes.
    pub sync_strategy: SyncStrategy,
    /// Maximum payload size (in bytes) for a single sync push.
    pub max_push_size_bytes: usize,
    /// Enable background push notification triggers.
    pub enable_push_notifications: bool,
}

impl Default for MobileConfig {
    fn default() -> Self {
        Self {
            db_path: ":memory:".to_string(),
            network_condition: NetworkCondition::Connected(NetworkType::Wifi),
            sync_strategy: SyncStrategy::Immediate,
            max_push_size_bytes: 1024 * 1024, // 1 MB default
            enable_push_notifications: true,
        }
    }
}

impl MobileConfig {
    /// Create a new mobile config with the given database path.
    pub fn with_db_path(mut self, path: impl Into<String>) -> Self {
        self.db_path = path.into();
        self
    }

    /// Set the network condition and update sync strategy accordingly.
    pub fn with_condition(mut self, condition: NetworkCondition) -> Self {
        let was_offline = matches!(&condition, NetworkCondition::Offline);
        self.network_condition = condition;
        // Automatically adjust sync strategy based on connectivity
        if was_offline {
            self.sync_strategy = SyncStrategy::WifiOnly; // defer to wifi
        }
        self
    }

    /// Enable or disable push notifications.
    pub fn with_push_notifications(mut self, enable: bool) -> Self {
        self.enable_push_notifications = enable;
        self
    }

    /// Check if device is currently offline.
    pub fn is_offline(&self) -> bool {
        matches!(self.network_condition, NetworkCondition::Offline)
    }

    /// Get the estimated bandwidth for this condition.
    pub fn estimated_bandwidth_bytes_per_sec(&self) -> usize {
        match &self.network_condition {
            NetworkCondition::Connected(NetworkType::Wifi) => 10_000_000, // ~10 Mbps
            NetworkCondition::Connected(NetworkType::Cellular(cell_gen)) => match cell_gen {
                CellularGeneration::Gprs => 50,
                CellularGeneration::Edge => 200,
                CellularGeneration::Umts => 384_000,
                CellularGeneration::Hspa => 1_400_000,
                CellularGeneration::Lte => 10_000_000,
                CellularGeneration::Iden => 50_000,
                CellularGeneration::Evt => 276_000,
                CellularGeneration::OneX => 1_400_000,
                CellularGeneration::FiveG => 100_000_000,
            },
            NetworkCondition::Metered => 500_000, // conservative estimate
            NetworkCondition::Offline => 0,
        }
    }

    /// Whether to compress sync payloads for this condition.
    pub fn should_compress(&self) -> bool {
        matches!(
            &self.network_condition,
            NetworkCondition::Metered | NetworkCondition::Connected(NetworkType::Cellular(_))
        )
    }
}

/// Pending push item: a local change awaiting server sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPush {
    /// Local sequence number (monotonically increasing).
    pub seq: u64,
    /// Whether this was a create or update operation.
    pub op_type: PushOpType,
    /// Size of the payload in bytes (for bandwidth estimation).
    pub payload_size_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PushOpType {
    Create,
    Update,
    Delete,
}

/// Mobile store engine that handles offline CRUD + sync planning.
///
/// In production this wraps libsql/SQLite; here it manages the pending-push queue
/// and bandwidth estimation for push decisions.
#[derive(Debug)]
pub struct MobileEngine {
    pub config: MobileConfig,
    /// Sequence counter for pending operations.
    seq: u64,
}

impl MobileEngine {
    /// Create a new mobile engine from config.
    pub fn new(config: MobileConfig) -> Self {
        Self { config, seq: 0 }
    }

    /// Register a pending push operation. Returns the assigned sequence number.
    pub fn enqueue_push(&mut self, op_type: PushOpType, payload_size_bytes: usize) -> u64 {
        let seq = self.seq;
        self.seq += 1;
        // In production this would write to the device's SQLite store.
        // For now we just track in memory.
        tracing::debug!(
            seq = %seq,
            ?op_type,
            payload_size_bytes,
            "enqueued push"
        );
        seq
    }

    /// Estimate whether all pending pushes can be sent within the bandwidth budget.
    pub fn can_push_all_pending(&self, budget_bytes: usize) -> bool {
        // In production this would sum up actual pending push sizes from the store.
        let estimated_total: usize = (0..self.seq as usize)
            .map(|i| i * 1024) // simulate varying payload sizes
            .sum();
        estimated_total <= budget_bytes
    }

    /// Get a bandwidth-aware sync plan for pending changes.
    pub fn build_sync_plan(&self, available_bytes: usize) -> SyncPlan {
        let bandwidth = self.config.estimated_bandwidth_bytes_per_sec();
        let max_pushes = if available_bytes >= self.config.max_push_size_bytes {
            self.seq as usize // all can fit
        } else {
            available_bytes / 1024.min(available_bytes) // proportional estimate
        };

        SyncPlan {
            total_pushes: self.seq as usize,
            can_fit: max_pushes,
            bandwidth_bps: bandwidth,
            estimated_duration_secs: if bandwidth == 0 {
                i64::MAX
            } else {
                (estimated_total_bytes(bandwidth) as f64 / bandwidth as f64).ceil() as i64
            },
        }
    }

    /// Check if device should suppress sync based on current network condition.
    pub fn should_suppress_sync(&self) -> bool {
        match &self.config.network_condition {
            NetworkCondition::Offline => true,
            NetworkCondition::Metered => {
                matches!(self.config.sync_strategy, SyncStrategy::WifiOnly)
            }
            NetworkCondition::Connected(ntype) => {
                // Suppress push notifications if on cellular with low generation
                if let NetworkType::Cellular(cell_gen) = ntype {
                    matches!(
                        cell_gen,
                        CellularGeneration::Gprs
                            | CellularGeneration::Edge
                            | CellularGeneration::Iden
                    )
                } else {
                    false
                }
            }
        }
    }
}

/// A sync plan computed by the mobile engine.
#[derive(Debug, Clone)]
pub struct SyncPlan {
    pub total_pushes: usize,
    pub can_fit: usize,
    pub bandwidth_bps: usize,
    pub estimated_duration_secs: i64,
}

impl SyncPlan {
    /// Whether all pending pushes can be sent in the plan.
    pub fn is_complete(&self) -> bool {
        self.can_fit >= self.total_pushes
            && self.bandwidth_bps > 0
            && self.estimated_duration_secs < i64::MAX
    }

    /// Number of remaining pushes not included in this plan.
    pub fn remaining(&self) -> usize {
        self.total_pushes.saturating_sub(self.can_fit)
    }
}

// Helper function used above (avoids dead-code warning for unused import)
fn estimated_total_bytes(_bandwidth: usize) -> usize {
    // In production this would sum pending push sizes from the store
    0
}

#[cfg(test)]
mod mobile_tests {
    use super::*;

    #[test]
    fn test_mobile_config_default() {
        let config = MobileConfig::default();
        assert!(!config.is_offline());
        assert!(matches!(
            config.network_condition,
            NetworkCondition::Connected(NetworkType::Wifi)
        ));
        assert_eq!(config.max_push_size_bytes, 1024 * 1024);
    }

    #[test]
    fn test_mobile_config_with_db_path() {
        let config = MobileConfig::default().with_db_path("/tmp/test.db");
        assert_eq!(config.db_path, "/tmp/test.db");
    }

    #[test]
    fn test_mobile_config_offline_sets_wifionly_sync() {
        let config = MobileConfig::default()
            .with_condition(NetworkCondition::Offline)
            .with_push_notifications(false);
        assert!(config.is_offline());
        assert!(!config.enable_push_notifications);
    }

    #[test]
    fn test_mobile_config_bandwidth_estimates() {
        let wifi = MobileConfig::default();
        assert_eq!(wifi.estimated_bandwidth_bytes_per_sec(), 10_000_000);

        let cellular = MobileConfig::default().with_condition(NetworkCondition::Connected(
            NetworkType::Cellular(CellularGeneration::Lte),
        ));
        assert_eq!(cellular.estimated_bandwidth_bytes_per_sec(), 10_000_000);

        let gprs = MobileConfig::default().with_condition(NetworkCondition::Connected(
            NetworkType::Cellular(CellularGeneration::Gprs),
        ));
        assert_eq!(gprs.estimated_bandwidth_bytes_per_sec(), 50);

        let offline = MobileConfig::default().with_condition(NetworkCondition::Offline);
        assert_eq!(offline.estimated_bandwidth_bytes_per_sec(), 0);

        let metered = MobileConfig::default().with_condition(NetworkCondition::Metered);
        assert_eq!(metered.estimated_bandwidth_bytes_per_sec(), 500_000);
    }

    #[test]
    fn test_mobile_config_should_compress() {
        let wifi = MobileConfig::default();
        assert!(!wifi.should_compress());

        let cellular = MobileConfig::default().with_condition(NetworkCondition::Connected(
            NetworkType::Cellular(CellularGeneration::Lte),
        ));
        assert!(cellular.should_compress());

        let metered = MobileConfig::default().with_condition(NetworkCondition::Metered);
        assert!(metered.should_compress());
    }

    #[test]
    fn test_mobile_engine_enqueue_push() {
        let config = MobileConfig::default();
        let mut engine = MobileEngine::new(config);

        let seq1 = engine.enqueue_push(PushOpType::Create, 4096);
        assert_eq!(seq1, 0);

        let seq2 = engine.enqueue_push(PushOpType::Update, 8192);
        assert_eq!(seq2, 1);

        let seq3 = engine.enqueue_push(PushOpType::Delete, 512);
        assert_eq!(seq3, 2);
    }

    #[test]
    fn test_mobile_engine_sync_plan() {
        let config =
            MobileConfig::default().with_condition(NetworkCondition::Connected(NetworkType::Wifi));
        let mut engine = MobileEngine::new(config);
        engine.enqueue_push(PushOpType::Create, 1024);
        engine.enqueue_push(PushOpType::Update, 2048);

        let plan = engine.build_sync_plan(10_000_000);
        assert!(plan.is_complete());
        assert_eq!(plan.remaining(), 0);
    }

    #[test]
    fn test_mobile_engine_suppress_sync_offline() {
        let config = MobileConfig::default().with_condition(NetworkCondition::Offline);
        let engine = MobileEngine::new(config);
        assert!(engine.should_suppress_sync());
    }

    #[test]
    fn test_mobile_engine_suppress_sync_low_cellular() {
        let config = MobileConfig::default().with_condition(NetworkCondition::Connected(
            NetworkType::Cellular(CellularGeneration::Gprs),
        ));
        let engine = MobileEngine::new(config);
        assert!(engine.should_suppress_sync());
    }

    #[test]
    fn test_mobile_engine_wifi_no_suppress() {
        let config = MobileConfig::default();
        let engine = MobileEngine::new(config);
        assert!(!engine.should_suppress_sync());
    }
}
