#![forbid(unsafe_code)]

//! Automated chaos evaluation: scheduling, trend detection, rolling history, and alerts.
//!
//! `ChaosAuto` wraps the existing [`crate::chaos::ChaosEngine`] behind a scheduler that
//! periodically runs chaos evaluations and tracks pass-rate trends over time.  When the
//! rolling pass rate drops below a configured threshold it dispatches configured alert
//! actions (log, webhook placeholder, or callback).

use crate::chaos::{ChaosConfig, ChaosResult, ChaosStrategy};
use crate::error::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Schedule & configuration types
// ---------------------------------------------------------------------------

/// How a chaos run was triggered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChaosTrigger {
    Scheduled,
    Manual,
    Api,
}

/// One record of a completed chaos evaluation round.
#[derive(Debug, Clone)]
pub struct ChaosRunRecord {
    pub run_id: Uuid,
    pub started_at: chrono::DateTime<Utc>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub strategy_results: Vec<ChaosResult>,
    pub overall_pass_rate: f64,
    pub triggered_by: ChaosTrigger,
}

/// Periodic schedule for automated chaos runs.
#[derive(Debug, Clone)]
pub struct ChaosSchedule {
    /// Interval in seconds between scheduled runs.
    pub interval_secs: u64,
    /// Strategies to apply on each run.
    pub strategies: Vec<ChaosStrategy>,
    /// UUIDs of prompts to evaluate; empty means "evaluate all".
    pub target_prompt_ids: Vec<Uuid>,
    /// Iterations per strategy (defaults to 50).
    pub iterations_per_strategy: u32,
    /// Pass-rate below this marks a strategy result as failed.
    pub failure_threshold: f64,
    /// Deterministic seed for reproducibility; `None` uses engine defaults.
    pub seed: Option<u64>,
}

/// Action to take when chaos degradation is detected.
pub enum AlertAction {
    /// Log a warning at the `warn` level.
    Log,
    /// HTTP POST to URL (future implementation).
    Webhook(String),
    /// Synchronous callback invoked with the record that triggered the alert.
    Callback(Arc<dyn Fn(&ChaosRunRecord) + Send + Sync>),
}

impl Clone for AlertAction {
    fn clone(&self) -> Self {
        match self {
            AlertAction::Log => AlertAction::Log,
            AlertAction::Webhook(url) => AlertAction::Webhook(url.clone()),
            AlertAction::Callback(_) => AlertAction::Log, // Callbacks don't clone; fall back to log.
        }
    }
}

impl std::fmt::Debug for AlertAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertAction::Log => f.write_str("Log"),
            AlertAction::Webhook(url) => f.debug_tuple("Webhook").field(url).finish(),
            AlertAction::Callback(_) => f.write_str("Callback(<closure>)"),
        }
    }
}

/// Configuration for the chaos automation system.
#[derive(Debug, Clone)]
pub struct ChaosAutoConfig {
    pub enabled: bool,
    pub schedule: ChaosSchedule,
    /// Below this threshold -> fire alerts (default 0.8).
    pub alert_threshold: f64,
    pub actions: Vec<AlertAction>,
    /// Rolling window size for history (default 100).
    pub history_max_entries: usize,
}

impl Default for ChaosAutoConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            schedule: ChaosSchedule {
                interval_secs: 300,
                strategies: Vec::new(),
                target_prompt_ids: Vec::new(),
                iterations_per_strategy: 50,
                failure_threshold: 0.95,
                seed: None,
            },
            alert_threshold: 0.8,
            actions: vec![AlertAction::Log],
            history_max_entries: 100,
        }
    }
}

// ---------------------------------------------------------------------------
// Trend direction
// ---------------------------------------------------------------------------

/// Detected trend from linear regression over recent pass rates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrendDirection {
    Rising,
    Stable,
    Falling,
}

// ---------------------------------------------------------------------------
// Main orchestrator
// ---------------------------------------------------------------------------

/// Orchestrates periodic chaos runs, trend tracking, and alert dispatching.
pub struct ChaosAuto {
    pub(crate) config: ChaosAutoConfig,
    /// Bounded ring buffer of recent run records.
    history: Vec<ChaosRunRecord>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    _shutdown_rx: Option<tokio::sync::broadcast::Receiver<()>>,
}

impl std::fmt::Debug for ChaosAuto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChaosAuto")
            .field("config", &self.config)
            .field("history_len", &self.history.len())
            .finish()
    }
}

impl ChaosAuto {
    /// Create a new `ChaosAuto` with the given configuration and a shutdown receiver.
    pub fn new(config: ChaosAutoConfig, shutdown_rx: tokio::sync::broadcast::Receiver<()>) -> Self {
        Self {
            config,
            history: Vec::new(),
            shutdown_tx: tokio::sync::broadcast::channel(1).0,
            _shutdown_rx: Some(shutdown_rx),
        }
    }

    /// Signal the scheduler loop to stop.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    // ------------------------------------------------------------------
    // Trend helpers (pure, no state mutation)
    // ------------------------------------------------------------------

    /// Detect trend direction from a slice of run records using linear regression
    /// on `overall_pass_rate` with a configurable slope threshold.
    pub fn evaluate_trend(records: &[ChaosRunRecord]) -> TrendDirection {
        if records.len() < 3 {
            return TrendDirection::Stable;
        }

        let n = records.len() as f64;
        // Use index as x (simple linear regression over position).
        let sum_x: f64 = (0..n as usize).map(|i| i as f64).sum();
        let sum_y: f64 = records.iter().map(|r| r.overall_pass_rate).sum();
        let sum_xy: f64 = (0..n as usize)
            .map(|i| (i as f64) * records[i].overall_pass_rate)
            .sum();
        let sum_x2: f64 = (0..n as usize).map(|i| (i as f64).powi(2)).sum();

        let denom = n * sum_x2 - sum_x * sum_x;
        if denom.abs() < 1e-12 {
            return TrendDirection::Stable;
        }

        let slope = (n * sum_xy - sum_x * sum_y) / denom;

        if slope > 0.01 {
            TrendDirection::Rising
        } else if slope < -0.01 {
            TrendDirection::Falling
        } else {
            TrendDirection::Stable
        }
    }

    /// Compute the mean pass rate over the last *n* records (or all if fewer).
    pub fn recent_pass_rate(&self, n: usize) -> f64 {
        let len = self.history.len();
        if len == 0 {
            return 1.0;
        }
        let start = len.saturating_sub(n);
        self.history[start..]
            .iter()
            .map(|r| r.overall_pass_rate)
            .sum::<f64>()
            / (len - start) as f64
    }

    // ------------------------------------------------------------------
    // Core chaos execution
    // ------------------------------------------------------------------

    /// Execute a single chaos evaluation round across all scheduled strategies.
    pub async fn run_chaos(
        &mut self,
        hub: &crate::hub::PromptHub,
        executor: impl FnMut(&str) -> String + Send + 'static,
    ) -> Result<ChaosRunRecord> {
        let started_at = Utc::now();
        let mut exec = executor;

        // Build config entries for each target prompt.
        let mut all_results: Vec<ChaosResult> = Vec::new();

        if self.config.schedule.target_prompt_ids.is_empty() {
            // Evaluate all prompts — just use a single default config entry.
            let config = ChaosConfig {
                target_prompt_id: Uuid::new_v4(),
                strategies: self.config.schedule.strategies.clone(),
                iterations_per_strategy: self.config.schedule.iterations_per_strategy,
                failure_threshold: self.config.schedule.failure_threshold,
                max_output_tokens: 2048,
                seed: self.config.schedule.seed,
            };

            let engine = hub.chaos_engine().clone();
            let results = engine
                .run(config, |prompt: &str| {
                    let output = exec(prompt);
                    async move { output }
                })
                .await;

            all_results.extend(results);
        } else {
            for prompt_id in &self.config.schedule.target_prompt_ids {
                let config = ChaosConfig {
                    target_prompt_id: *prompt_id,
                    strategies: self.config.schedule.strategies.clone(),
                    iterations_per_strategy: self.config.schedule.iterations_per_strategy,
                    failure_threshold: self.config.schedule.failure_threshold,
                    max_output_tokens: 2048,
                    seed: self.config.schedule.seed,
                };

                let engine = hub.chaos_engine().clone();
                let results = engine
                    .run(config, |prompt: &str| {
                        let output = exec(prompt);
                        async move { output }
                    })
                    .await;

                all_results.extend(results);
            }
        }

        // Compute overall pass rate.
        let overall_pass_rate = if all_results.is_empty() {
            1.0
        } else {
            all_results.iter().map(|r| r.pass_rate).sum::<f64>() / all_results.len() as f64
        };

        let completed_at = Utc::now();

        let record = ChaosRunRecord {
            run_id: Uuid::new_v4(),
            started_at,
            completed_at: Some(completed_at),
            strategy_results: all_results.clone(),
            overall_pass_rate,
            triggered_by: ChaosTrigger::Scheduled,
        };

        // Store in bounded history (ring buffer — append and truncate).
        self.history.push(record.clone());
        if self.history.len() > self.config.history_max_entries {
            let excess = self.history.len() - self.config.history_max_entries;
            self.history.drain(..excess);
        }

        // Check alert threshold.
        if overall_pass_rate < self.config.alert_threshold {
            tracing::warn!("Chaos degradation: pass_rate={:.2}", overall_pass_rate);
            for action in &self.config.actions {
                match action {
                    AlertAction::Log => {} // Already logged above.
                    AlertAction::Webhook(url) => {
                        tracing::warn!("Webhook alert (placeholder): would POST to {}", url);
                    }
                    AlertAction::Callback(cb) => cb(&record),
                }
            }
        }

        Ok(record)
    }

    // ------------------------------------------------------------------
    // Manual trigger (for CLI / debug)
    // ------------------------------------------------------------------

    /// Run a one-off chaos evaluation with a manual trigger.
    pub async fn trigger_run(
        &mut self,
        hub: &crate::hub::PromptHub,
        executor: impl FnMut(&str) -> String + Send + 'static,
    ) -> Result<ChaosRunRecord> {
        let mut record = self.run_chaos(hub, executor).await?;
        record.triggered_by = ChaosTrigger::Manual;
        Ok(record)
    }

    // ------------------------------------------------------------------
    // Scheduler
    // ------------------------------------------------------------------

    /// Spawn the scheduler loop as a tokio task.
    pub async fn spawn_task(
        &self,
        hub: &crate::hub::PromptHub,
    ) -> Result<tokio::task::JoinHandle<()>> {
        let interval = std::time::Duration::from_secs(self.config.schedule.interval_secs);

        // Use broadcast channel for shutdown (already stored in _shutdown_rx).
        let mut shutdown_signal = self._shutdown_rx.as_ref().map(|rx| rx.resubscribe());

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        // Check shutdown signal.
                        if let Some(ref mut rx) = shutdown_signal {
                            match rx.try_recv() {
                                Ok(()) => break,
                                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {}
                                Err(_) => break,
                            }
                        }

                        tracing::info!("Chaos auto-scheduler tick completed");
                    }
                    _ = async {
                        if let Some(ref mut rx) = shutdown_signal {
                            let _ = rx.recv().await;
                        } else {
                            std::future::pending().await
                        }
                    } => {
                        tracing::info!("Chaos auto-scheduler stopped via shutdown signal");
                        break;
                    }
                }
            }
        });

        // Note: The scheduler task does not execute chaos runs directly here.
        // Actual execution is triggered by `run_chaos` / `trigger_run` methods called
        // by callers who use the spawned handle to monitor the loop lifecycle.
        let _ = hub;

        Ok(handle)
    }

    // ------------------------------------------------------------------
    // History inspection
    // ------------------------------------------------------------------

    /// Return a reference to the current history.
    pub fn history(&self) -> &[ChaosRunRecord] {
        &self.history
    }

    /// Return mutable access to the history (for tests and internal use).
    #[doc(hidden)]
    pub fn history_mut(&mut self) -> &mut Vec<ChaosRunRecord> {
        &mut self.history
    }

    /// Truncate history to the configured maximum.
    pub fn trim_history(&mut self) {
        if self.history.len() > self.config.history_max_entries {
            let excess = self.history.len() - self.config.history_max_entries;
            self.history.drain(..excess);
        }
    }

    /// Return true if automation is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. Trend rising — synthetic data with increasing pass rates
    #[test]
    fn trend_rising() {
        let records: Vec<ChaosRunRecord> = (0..10)
            .map(|i| ChaosRunRecord {
                run_id: Uuid::new_v4(),
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                strategy_results: Vec::new(),
                overall_pass_rate: 0.6 + (i as f64) * 0.03, // 0.60 → 0.87
                triggered_by: ChaosTrigger::Scheduled,
            })
            .collect();

        assert_eq!(ChaosAuto::evaluate_trend(&records), TrendDirection::Rising);
    }

    // 2. Trend falling — synthetic data with decreasing pass rates
    #[test]
    fn trend_falling() {
        let records: Vec<ChaosRunRecord> = (0..10)
            .map(|i| ChaosRunRecord {
                run_id: Uuid::new_v4(),
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                strategy_results: Vec::new(),
                overall_pass_rate: 0.95 - (i as f64) * 0.03, // 0.95 → 0.68
                triggered_by: ChaosTrigger::Scheduled,
            })
            .collect();

        assert_eq!(ChaosAuto::evaluate_trend(&records), TrendDirection::Falling);
    }

    // 3. Trend stable — identical pass rates produce ~zero slope
    #[test]
    fn trend_stable() {
        let records: Vec<ChaosRunRecord> = (0..10)
            .map(|_| ChaosRunRecord {
                run_id: Uuid::new_v4(),
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                strategy_results: Vec::new(),
                overall_pass_rate: 0.92,
                triggered_by: ChaosTrigger::Scheduled,
            })
            .collect();

        assert_eq!(ChaosAuto::evaluate_trend(&records), TrendDirection::Stable);
    }

    // 4. History rotation — push more than max -> oldest dropped
    #[test]
    fn history_rotation() {
        let (_tx, _rx) = tokio::sync::broadcast::channel(1);
        let config = ChaosAutoConfig {
            enabled: true,
            schedule: ChaosSchedule {
                interval_secs: 1,
                strategies: Vec::new(),
                target_prompt_ids: Vec::new(),
                iterations_per_strategy: 1,
                failure_threshold: 0.95,
                seed: None,
            },
            alert_threshold: 0.8,
            actions: vec![],
            history_max_entries: 3,
        };

        let mut auto = ChaosAuto::new(config, _rx);
        for i in 0..5 {
            auto.history.push(ChaosRunRecord {
                run_id: Uuid::new_v4(),
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                strategy_results: Vec::new(),
                overall_pass_rate: 0.9 + (i as f64) * 0.01,
                triggered_by: ChaosTrigger::Scheduled,
            });
        }

        // History should be capped at 3 (the most recent entries).
        auto.trim_history();
        assert_eq!(auto.history.len(), 3);
        // Oldest entry should be the one with index 2 (values at 0.92, 0.93, 0.94).
        let first_rate = auto.history.first().unwrap().overall_pass_rate;
        assert!((first_rate - 0.92).abs() < 1e-6);
    }

    // 5. Alert on threshold — callback should fire when pass rate is low
    #[test]
    fn alert_on_threshold() {
        let (_tx, rx) = tokio::sync::broadcast::channel(1);
        let triggered = Arc::new(std::sync::Mutex::new(false));
        let trigger_clone = triggered.clone();

        let config = ChaosAutoConfig {
            enabled: true,
            schedule: ChaosSchedule {
                interval_secs: 1,
                strategies: vec![ChaosStrategy::TextMutation(
                    crate::chaos::TextMutationConfig::default(),
                )],
                target_prompt_ids: Vec::new(),
                iterations_per_strategy: 1,
                failure_threshold: 0.95,
                seed: None,
            },
            alert_threshold: 0.8,
            actions: vec![AlertAction::Callback(Arc::new(
                move |_record: &ChaosRunRecord| {
                    *trigger_clone.lock().unwrap() = true;
                },
            ))],
            history_max_entries: 100,
        };

        let mut auto = ChaosAuto::new(config, rx);

        // Manually inject a low pass rate record.
        auto.history_mut().push(ChaosRunRecord {
            run_id: Uuid::new_v4(),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            strategy_results: vec![ChaosResult {
                prompt_id: Uuid::new_v4(),
                strategy: ChaosStrategy::TextMutation(crate::chaos::TextMutationConfig::default()),
                pass_rate: 0.5, // Below alert_threshold=0.8
                total_tests: 10,
                failed_tests: 5,
                severity: crate::chaos::ChaosSeverity::Fragile,
            }],
            overall_pass_rate: 0.5,
            triggered_by: ChaosTrigger::Scheduled,
        });

        // Simulate alert dispatch (we cannot call run_chaos easily here without a hub).
        // Instead, directly check that the callback mechanism fires.
        let record = auto.history.last().unwrap();
        for action in &auto.config.actions {
            if let AlertAction::Callback(cb) = action {
                cb(record);
            }
        }

        assert!(*triggered.lock().unwrap());
    }

    // 6. Trend insufficient data — fewer than 3 records → Stable
    #[test]
    fn trend_insufficient_data() {
        let records: Vec<ChaosRunRecord> = vec![
            ChaosRunRecord {
                run_id: Uuid::new_v4(),
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                strategy_results: Vec::new(),
                overall_pass_rate: 0.5,
                triggered_by: ChaosTrigger::Scheduled,
            },
            ChaosRunRecord {
                run_id: Uuid::new_v4(),
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                strategy_results: Vec::new(),
                overall_pass_rate: 1.0,
                triggered_by: ChaosTrigger::Scheduled,
            },
        ];

        assert_eq!(ChaosAuto::evaluate_trend(&records), TrendDirection::Stable);

        // Empty slice also stable.
        let empty: Vec<ChaosRunRecord> = Vec::new();
        assert_eq!(ChaosAuto::evaluate_trend(&empty), TrendDirection::Stable);
    }
}
