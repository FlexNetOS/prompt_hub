#![forbid(unsafe_code)]

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tracing::{info, instrument, warn};

/// Monthly budget tracker with alert thresholds.
///
/// Tracks spend against a configured budget and fires alerts when
/// configurable thresholds are crossed (50%, 80%, 100%).
#[derive(Debug)]
pub struct BudgetTracker {
    monthly_budget_usd: AtomicU64,
    current_spend_micros: AtomicU64,
    alert_threshold_percent: AtomicU64,
    fifty_percent_alerted: AtomicBool,
    eighty_percent_alerted: AtomicBool,
    hundred_percent_alerted: AtomicBool,
}

/// Budget configuration persisted per organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub monthly_budget_usd: f64,
    pub alert_threshold_percent: f64,
    pub org_id: String,
}

/// Alert level triggered by budget threshold crossing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetAlert {
    None,
    FiftyPercent,
    EightyPercent,
    HundredPercent,
    OverBudget,
}

impl BudgetTracker {
    /// Create a new budget tracker with the given monthly budget.
    pub fn new(monthly_budget_usd: f64) -> Self {
        Self {
            monthly_budget_usd: AtomicU64::new((monthly_budget_usd * 1_000_000.0) as u64),
            current_spend_micros: AtomicU64::new(0),
            alert_threshold_percent: AtomicU64::new(80),
            fifty_percent_alerted: AtomicBool::new(false),
            eighty_percent_alerted: AtomicBool::new(false),
            hundred_percent_alerted: AtomicBool::new(false),
        }
    }

    /// Record a spend amount in USD.
    #[instrument(skip(self), fields(amount = amount_usd))]
    pub fn record_spend(&self, amount_usd: f64) -> BudgetAlert {
        let micros = (amount_usd * 1_000_000.0) as u64;
        let previous = self.current_spend_micros.fetch_add(micros, Ordering::SeqCst);
        let total = previous + micros;
        let budget = self.monthly_budget_usd.load(Ordering::SeqCst);

        if budget == 0 {
            return BudgetAlert::None;
        }

        let percent = (total * 100) / budget;
        let alert_threshold = self.alert_threshold_percent.load(Ordering::SeqCst);

        if percent >= 100 && !self.hundred_percent_alerted.swap(true, Ordering::SeqCst) {
            warn!("Budget 100% exceeded: {}%", percent);
            return BudgetAlert::HundredPercent;
        }
        if percent >= alert_threshold
            && !self.eighty_percent_alerted.swap(true, Ordering::SeqCst)
        {
            warn!("Budget {}% threshold crossed: {}%", alert_threshold, percent);
            return BudgetAlert::EightyPercent;
        }
        if percent >= 50 && !self.fifty_percent_alerted.swap(true, Ordering::SeqCst) {
            info!("Budget 50% threshold crossed: {}%", percent);
            return BudgetAlert::FiftyPercent;
        }

        BudgetAlert::None
    }

    /// Get current spend as USD.
    pub fn current_spend_usd(&self) -> f64 {
        let micros = self.current_spend_micros.load(Ordering::SeqCst);
        micros as f64 / 1_000_000.0
    }

    /// Get budget utilization as a percentage (0.0 to 100.0+).
    pub fn utilization_percent(&self) -> f64 {
        let budget = self.monthly_budget_usd.load(Ordering::SeqCst);
        if budget == 0 {
            return 0.0;
        }
        let spent = self.current_spend_micros.load(Ordering::SeqCst);
        (spent as f64 / budget as f64) * 100.0
    }

    /// Check if the budget is exceeded.
    pub fn is_exceeded(&self) -> bool {
        let budget = self.monthly_budget_usd.load(Ordering::SeqCst);
        let spent = self.current_spend_micros.load(Ordering::SeqCst);
        spent > budget
    }

    /// Reset spend counters for a new billing period.
    #[instrument(skip(self))]
    pub fn reset_period(&self) {
        self.current_spend_micros.store(0, Ordering::SeqCst);
        self.fifty_percent_alerted.store(false, Ordering::SeqCst);
        self.eighty_percent_alerted.store(false, Ordering::SeqCst);
        self.hundred_percent_alerted.store(false, Ordering::SeqCst);
        info!("Budget period reset");
    }

    /// Update the monthly budget amount.
    pub fn set_budget(&self, monthly_budget_usd: f64) {
        let micros = (monthly_budget_usd * 1_000_000.0) as u64;
        self.monthly_budget_usd.store(micros, Ordering::SeqCst);
    }

    /// Load budget configuration.
    pub fn load_config(&self, config: &BudgetConfig) -> Result<()> {
        self.set_budget(config.monthly_budget_usd);
        let threshold_micros = (config.alert_threshold_percent * 1_000_000.0) as u64;
        self.alert_threshold_percent
            .store((threshold_micros / 1_000_000) as u64, Ordering::SeqCst);
        Ok(())
    }

    /// Save current budget state to configuration.
    pub fn save_config(&self, org_id: &str) -> Result<BudgetConfig> {
        Ok(BudgetConfig {
            monthly_budget_usd: self.monthly_budget_usd.load(Ordering::SeqCst) as f64 / 1_000_000.0,
            alert_threshold_percent: self.alert_threshold_percent.load(Ordering::SeqCst) as f64,
            org_id: org_id.to_string(),
        })
    }
}

impl Default for BudgetTracker {
    fn default() -> Self {
        Self::new(1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_new() {
        let bt = BudgetTracker::new(500.0);
        assert!(!bt.is_exceeded());
        assert_eq!(bt.current_spend_usd(), 0.0);
    }

    #[test]
    fn test_record_spend() {
        let bt = BudgetTracker::new(100.0);
        let alert = bt.record_spend(25.0);
        assert_eq!(alert, BudgetAlert::None);
        assert_eq!(bt.current_spend_usd(), 25.0);
    }

    #[test]
    fn test_fifty_percent_alert() {
        let bt = BudgetTracker::new(100.0);
        let alert = bt.record_spend(50.0);
        assert_eq!(alert, BudgetAlert::FiftyPercent);
    }

    #[test]
    fn test_eighty_percent_alert() {
        let bt = BudgetTracker::new(100.0);
        let alert = bt.record_spend(85.0);
        assert_eq!(alert, BudgetAlert::EightyPercent);
    }

    #[test]
    fn test_hundred_percent_alert() {
        let bt = BudgetTracker::new(100.0);
        let alert = bt.record_spend(101.0);
        assert_eq!(alert, BudgetAlert::HundredPercent);
    }

    #[test]
    fn test_utilization_percent() {
        let bt = BudgetTracker::new(200.0);
        bt.record_spend(50.0);
        assert_eq!(bt.utilization_percent(), 25.0);
    }

    #[test]
    fn test_is_exceeded() {
        let bt = BudgetTracker::new(100.0);
        bt.record_spend(150.0);
        assert!(bt.is_exceeded());
    }

    #[test]
    fn test_reset_period() {
        let bt = BudgetTracker::new(100.0);
        bt.record_spend(50.0);
        assert_eq!(bt.current_spend_usd(), 50.0);
        bt.reset_period();
        assert_eq!(bt.current_spend_usd(), 0.0);
    }

    #[test]
    fn test_set_budget() {
        let bt = BudgetTracker::new(100.0);
        bt.record_spend(150.0);
        assert!(bt.is_exceeded());
        bt.set_budget(500.0);
        assert!(!bt.is_exceeded());
    }

    #[test]
    fn test_default() {
        let bt = BudgetTracker::default();
        assert!(!bt.is_exceeded());
    }

    #[test]
    fn test_config_roundtrip() {
        let bt = BudgetTracker::new(100.0);
        let config = bt.save_config("org-123").unwrap();
        assert_eq!(config.monthly_budget_usd, 100.0);
        assert_eq!(config.org_id, "org-123");

        let bt2 = BudgetTracker::new(0.0);
        bt2.load_config(&config).unwrap();
        assert_eq!(bt2.current_spend_usd(), 0.0);
    }
}
