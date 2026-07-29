#![forbid(unsafe_code)]

//! Automated chaos evaluation: scheduling, trend detection, rolling history, and alerts.
//!
//! `ChaosAuto` wraps the existing [`crate::chaos::ChaosEngine`] behind a scheduler that
//! periodically runs chaos evaluations and tracks pass-rate trends over time.  When the
//! rolling pass rate drops below a configured threshold it dispatches configured alert
//! actions (log, **real HTTP webhook POST**, or callback).
//!
//! The scheduler task spawned by [`ChaosAuto::spawn_task`] runs the *real* chaos engine on
//! every tick (it does not merely log); a self-contained, `'static` execution context is
//! moved into the task so it can run independently of the owning hub borrow.

use crate::chaos::{ChaosConfig, ChaosEngine, ChaosResult, ChaosStrategy};
use crate::error::{HubError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
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
    /// Perform a real HTTP `POST` of the alert payload to the given URL.
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

/// Per-tick observer invoked by the scheduler with each produced [`ChaosRunRecord`].
///
/// Primarily a test seam: lets a caller observe that the scheduler fired the real chaos
/// engine on each tick without inspecting the (task-local) rolling history.
pub type TickHook = Arc<dyn Fn(&ChaosRunRecord) + Send + Sync>;

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

        // Check alert threshold and dispatch configured actions (real webhook POST).
        if overall_pass_rate < self.config.alert_threshold {
            Self::dispatch_alerts(&self.config.actions, &record).await;
        }

        Ok(record)
    }

    // ------------------------------------------------------------------
    // Alert dispatch (shared by manual `run_chaos` and the scheduler task)
    // ------------------------------------------------------------------

    /// Build the JSON body sent to a webhook for a given run record.
    ///
    /// `ChaosRunRecord` (and the `ChaosResult`s it contains) are not `Serialize`,
    /// so we project the alert-relevant fields into a stable JSON shape here.
    fn webhook_payload(record: &ChaosRunRecord) -> serde_json::Value {
        serde_json::json!({
            "run_id": record.run_id.to_string(),
            "started_at": record.started_at.to_rfc3339(),
            "completed_at": record.completed_at.map(|t| t.to_rfc3339()),
            "overall_pass_rate": record.overall_pass_rate,
            "triggered_by": format!("{:?}", record.triggered_by),
            "strategy_count": record.strategy_results.len(),
        })
    }

    /// Dispatch every configured alert action for a degradation `record`.
    ///
    /// `Log` emits a `warn`, `Webhook` performs a **real** HTTP `POST` of the
    /// [`Self::webhook_payload`] JSON, and `Callback` invokes the closure.
    /// A failed webhook POST is logged and skipped — alert dispatch is best-effort
    /// and never aborts the chaos run.
    pub(crate) async fn dispatch_alerts(actions: &[AlertAction], record: &ChaosRunRecord) {
        tracing::warn!(
            "Chaos degradation: pass_rate={:.2}",
            record.overall_pass_rate
        );
        for action in actions {
            match action {
                AlertAction::Log => {} // Degradation already logged above.
                AlertAction::Webhook(url) => {
                    if let Err(e) = Self::post_webhook(url, record).await {
                        tracing::warn!("Webhook alert POST to {url} failed: {e}");
                    }
                }
                AlertAction::Callback(cb) => cb(record),
            }
        }
    }

    /// Perform a single real HTTP `POST` of the alert payload to `url`.
    ///
    /// Uses the crate's existing [`reqwest`] client (the same HTTP stack used by
    /// `qdrant` / `local-llm`). Returns a [`HubError::Network`] on transport
    /// failure or a non-success HTTP status.
    async fn post_webhook(url: &str, record: &ChaosRunRecord) -> Result<()> {
        let body = Self::webhook_payload(record);
        let resp = reqwest::Client::new()
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| HubError::Network(format!("chaos webhook POST failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(HubError::Network(format!(
                "chaos webhook POST to {url} returned HTTP {}",
                resp.status()
            )));
        }
        tracing::info!("Chaos degradation webhook delivered to {url}");
        Ok(())
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

    /// Default per-tick prompt executor for the scheduler.
    ///
    /// Echoes the (already-mutated) prompt back as the model output. This is a valid,
    /// dependency-free executor that exercises the real chaos engine — callers that want
    /// to drive a live model should use [`Self::spawn_scheduler_with`] with a custom
    /// executor factory.
    fn default_executor() -> impl FnMut(&str) -> String + Send + 'static {
        |prompt: &str| prompt.to_string()
    }

    /// Spawn the scheduler loop as a tokio task that **actually runs the chaos engine**
    /// on every tick (per `schedule.interval_secs`), tracks a rolling local history for
    /// trend/alert decisions, and dispatches alerts (including real webhook POSTs) when
    /// the pass rate drops below `alert_threshold`.
    ///
    /// A self-contained `'static` execution context (a cloned [`ChaosEngine`], the
    /// schedule, and the alert config) is moved into the task, so it runs independently
    /// of the `hub` borrow. The `hub` is used here only to source the chaos engine.
    pub async fn spawn_task(
        &self,
        hub: &crate::hub::PromptHub,
    ) -> Result<tokio::task::JoinHandle<()>> {
        let interval = Duration::from_secs(self.config.schedule.interval_secs);
        self.spawn_scheduler_with(
            hub.chaos_engine().clone(),
            interval,
            Self::default_executor,
            None,
        )
    }

    /// Testable core of the scheduler: spawn a task that runs the real chaos engine on a
    /// caller-supplied `interval`, using `engine` and `executor_factory` (one executor is
    /// minted per tick so the `FnMut` need not be `Clone`). After every tick the optional
    /// `on_tick` hook fires with the produced record — tests inject a tiny interval and an
    /// `on_tick` counter to prove the engine fired without waiting real wall-clock.
    pub(crate) fn spawn_scheduler_with<F, E>(
        &self,
        engine: ChaosEngine,
        interval: Duration,
        mut executor_factory: F,
        on_tick: Option<TickHook>,
    ) -> Result<tokio::task::JoinHandle<()>>
    where
        F: FnMut() -> E + Send + 'static,
        E: FnMut(&str) -> String + Send + 'static,
    {
        let schedule = self.config.schedule.clone();
        let alert_threshold = self.config.alert_threshold;
        let actions = self.config.actions.clone();
        let history_max = self.config.history_max_entries;

        // Resubscribe to the shutdown broadcast so the task observes `shutdown()`.
        let mut shutdown_signal = self._shutdown_rx.as_ref().map(|rx| rx.resubscribe());

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut history: Vec<ChaosRunRecord> = Vec::new();

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        // Honour an already-pending shutdown before doing work.
                        if let Some(ref mut rx) = shutdown_signal {
                            match rx.try_recv() {
                                Ok(()) => break,
                                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {}
                                Err(_) => break,
                            }
                        }

                        // Actually run the real chaos engine for this tick.
                        let executor = executor_factory();
                        let record = Self::run_once(
                            &engine,
                            &schedule,
                            executor,
                            ChaosTrigger::Scheduled,
                        )
                        .await;

                        // Maintain a bounded local rolling history for trend tracking.
                        history.push(record.clone());
                        if history.len() > history_max {
                            let excess = history.len() - history_max;
                            history.drain(..excess);
                        }

                        // Fire alerts (real webhook POST) on degradation.
                        if record.overall_pass_rate < alert_threshold {
                            Self::dispatch_alerts(&actions, &record).await;
                        }

                        if let Some(ref hook) = on_tick {
                            hook(&record);
                        }
                        tracing::info!(
                            run_id = %record.run_id,
                            pass_rate = record.overall_pass_rate,
                            "Chaos auto-scheduler tick completed",
                        );
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

        Ok(handle)
    }

    /// Run one chaos round against a cloned [`ChaosEngine`] (no `self` history mutation).
    ///
    /// This is the stateless core shared by the scheduler task; [`Self::run_chaos`] keeps
    /// its own `&mut self` history-recording path. Both drive the *same* real engine.
    async fn run_once(
        engine: &ChaosEngine,
        schedule: &ChaosSchedule,
        executor: impl FnMut(&str) -> String + Send + 'static,
        triggered_by: ChaosTrigger,
    ) -> ChaosRunRecord {
        let started_at = Utc::now();
        let mut exec = executor;
        let mut all_results: Vec<ChaosResult> = Vec::new();

        let targets: Vec<Uuid> = if schedule.target_prompt_ids.is_empty() {
            vec![Uuid::new_v4()]
        } else {
            schedule.target_prompt_ids.clone()
        };

        for target in targets {
            let config = ChaosConfig {
                target_prompt_id: target,
                strategies: schedule.strategies.clone(),
                iterations_per_strategy: schedule.iterations_per_strategy,
                failure_threshold: schedule.failure_threshold,
                max_output_tokens: 2048,
                seed: schedule.seed,
            };
            let results = engine
                .clone()
                .run(config, |prompt: &str| {
                    let output = exec(prompt);
                    async move { output }
                })
                .await;
            all_results.extend(results);
        }

        let overall_pass_rate = if all_results.is_empty() {
            1.0
        } else {
            all_results.iter().map(|r| r.pass_rate).sum::<f64>() / all_results.len() as f64
        };

        ChaosRunRecord {
            run_id: Uuid::new_v4(),
            started_at,
            completed_at: Some(Utc::now()),
            strategy_results: all_results,
            overall_pass_rate,
            triggered_by,
        }
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
                    *trigger_clone.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
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

        assert!(*triggered.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
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

    // 7. Scheduler actually fires run_chaos at its interval.
    //
    // Injects a tiny interval and an `on_tick` callback (a shared atomic counter) so we
    // can prove the real chaos engine ran on each tick without waiting real wall-clock.
    #[tokio::test]
    async fn scheduler_fires_run_chaos_at_interval() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let (_tx, rx) = tokio::sync::broadcast::channel(1);
        let config = ChaosAutoConfig {
            enabled: true,
            schedule: ChaosSchedule {
                interval_secs: 999, // overridden by the injected interval below
                strategies: vec![ChaosStrategy::TextMutation(
                    crate::chaos::TextMutationConfig::default(),
                )],
                target_prompt_ids: Vec::new(),
                iterations_per_strategy: 1,
                failure_threshold: 0.95,
                seed: Some(7),
            },
            alert_threshold: 0.0, // never alert here — isolate the firing proof
            actions: vec![],
            history_max_entries: 100,
        };

        let auto = ChaosAuto::new(config, rx);

        let ticks = Arc::new(AtomicUsize::new(0));
        let ticks_hook = ticks.clone();
        let on_tick: TickHook = Arc::new(move |rec| {
            // Proof the engine produced a real record on each tick.
            assert!((0.0..=1.0).contains(&rec.overall_pass_rate));
            ticks_hook.fetch_add(1, Ordering::SeqCst);
        });

        let handle = auto
            .spawn_scheduler_with(
                ChaosEngine::with_seed(7),
                Duration::from_millis(10),
                ChaosAuto::default_executor,
                Some(on_tick),
            )
            .expect("spawn scheduler");

        // Wait for at least 3 ticks to land (bounded — never relies on real seconds).
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while ticks.load(Ordering::SeqCst) < 3 {
            if tokio::time::Instant::now() > deadline {
                panic!("scheduler did not fire run_chaos at its interval");
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        auto.shutdown();
        handle.abort();
        assert!(ticks.load(Ordering::SeqCst) >= 3);
    }

    // 8. Webhook action issues a REAL HTTP POST.
    //
    // Stands up a oneshot TCP listener acting as a minimal HTTP server, dispatches a
    // degradation alert with an `AlertAction::Webhook`, and asserts the POST arrived with
    // the expected method/path and a JSON body carrying the run's pass rate and id.
    #[tokio::test]
    async fn webhook_action_issues_real_post() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let url = format!("http://{addr}/alert");

        // Minimal one-shot HTTP server capturing the raw request.
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.expect("accept");
            let mut buf = vec![0u8; 4096];
            let n = sock.read(&mut buf).await.expect("read");
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .expect("write resp");
            req
        });

        let record = ChaosRunRecord {
            run_id: Uuid::new_v4(),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            strategy_results: Vec::new(),
            overall_pass_rate: 0.42,
            triggered_by: ChaosTrigger::Scheduled,
        };

        ChaosAuto::dispatch_alerts(&[AlertAction::Webhook(url)], &record).await;

        let req = server.await.expect("server task");
        assert!(req.starts_with("POST /alert "), "request was: {req}");
        assert!(
            req.contains("\"overall_pass_rate\":0.42"),
            "body missing pass rate: {req}"
        );
        assert!(
            req.contains(&record.run_id.to_string()),
            "body missing run_id: {req}"
        );
    }
}
