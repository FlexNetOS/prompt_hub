#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, instrument, warn};

/// Day-bucket key format (`YYYY-MM-DD`, UTC) used to group analytics events
/// into the time-series consumed by [`Analytics::cost_trends`].
const DAY_BUCKET_FORMAT: &str = "%Y-%m-%d";

/// Usage analytics aggregator.
///
/// Tracks prompt usage, success rates, cost trends, and generates
/// reports with adoption metrics.
#[derive(Debug)]
pub struct Analytics {
    prompt_usage: HashMap<String, AtomicU64>,
    success_count: AtomicU64,
    failure_count: AtomicU64,
    create_count: AtomicU64,
    update_count: AtomicU64,
    total_tokens_consumed: AtomicU64,
    total_cost_micros: AtomicU64,
    active_users: HashMap<String, std::time::Instant>,
    daily_events: HashMap<String, Vec<AnalyticsEvent>>,
}

/// A single analytics event.
#[derive(Debug, Clone)]
pub struct AnalyticsEvent {
    pub event_type: EventType,
    pub prompt_id: String,
    pub user_id: String,
    pub tokens_used: u64,
    pub cost_micros: u64,
    pub success: bool,
    pub duration_ms: u64,
    /// When the event occurred (UTC). Used to bucket events into the
    /// [`Analytics::cost_trends`] time-series by day.
    pub timestamp: DateTime<Utc>,
}

/// Type of analytics event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    PromptCreate,
    PromptUse,
    PromptUpdate,
    PromptDelete,
    Search,
    Export,
}

/// Usage report.
#[derive(Debug, Clone)]
pub struct UsageReport {
    pub total_prompt_uses: u64,
    pub total_creates: u64,
    pub total_updates: u64,
    pub success_rate: f64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub unique_prompts_used: usize,
    pub top_prompts: Vec<(String, u64)>,
}

/// Cost trend data point.
#[derive(Debug, Clone)]
pub struct CostTrend {
    pub period: String,
    pub cost_usd: f64,
    pub token_count: u64,
    pub request_count: u64,
}

/// Adoption metrics.
#[derive(Debug, Clone)]
pub struct AdoptionMetrics {
    pub active_users: usize,
    pub total_prompts_created: u64,
    pub prompts_per_user: f64,
    pub feature_usage: HashMap<String, u64>,
}

impl Analytics {
    /// Create a new analytics aggregator.
    pub fn new() -> Self {
        Self {
            prompt_usage: HashMap::new(),
            success_count: AtomicU64::new(0),
            failure_count: AtomicU64::new(0),
            create_count: AtomicU64::new(0),
            update_count: AtomicU64::new(0),
            total_tokens_consumed: AtomicU64::new(0),
            total_cost_micros: AtomicU64::new(0),
            active_users: HashMap::new(),
            daily_events: HashMap::new(),
        }
    }

    /// Record an analytics event.
    #[instrument(skip(self), fields(event_type = ?event.event_type, prompt_id = %event.prompt_id))]
    pub fn record_event(&mut self, event: AnalyticsEvent) {
        // Track prompt usage / lifecycle counters
        match event.event_type {
            EventType::PromptUse => {
                self.prompt_usage
                    .entry(event.prompt_id.clone())
                    .or_insert_with(|| AtomicU64::new(0))
                    .fetch_add(1, Ordering::SeqCst);
            }
            EventType::PromptCreate => {
                self.create_count.fetch_add(1, Ordering::SeqCst);
            }
            EventType::PromptUpdate => {
                self.update_count.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }

        // Track success/failure
        if event.success {
            self.success_count.fetch_add(1, Ordering::SeqCst);
        } else {
            self.failure_count.fetch_add(1, Ordering::SeqCst);
        }

        // Track tokens and cost
        self.total_tokens_consumed
            .fetch_add(event.tokens_used, Ordering::SeqCst);
        self.total_cost_micros
            .fetch_add(event.cost_micros, Ordering::SeqCst);

        // Track active user
        self.active_users
            .insert(event.user_id.clone(), std::time::Instant::now());

        // Store event in its UTC day bucket (drives the cost-trends series).
        let day_key = event.timestamp.format(DAY_BUCKET_FORMAT).to_string();
        self.daily_events.entry(day_key).or_default().push(event);
    }

    /// Generate a usage report.
    pub fn usage_report(&self) -> UsageReport {
        let total_uses: u64 = self
            .prompt_usage
            .values()
            .map(|v| v.load(Ordering::SeqCst))
            .sum();

        let successes = self.success_count.load(Ordering::SeqCst);
        let failures = self.failure_count.load(Ordering::SeqCst);
        let total = successes + failures;
        let success_rate = if total > 0 {
            (successes as f64 * 100.0) / total as f64
        } else {
            0.0
        };

        let mut top_prompts: Vec<(String, u64)> = self
            .prompt_usage
            .iter()
            .map(|(k, v)| (k.clone(), v.load(Ordering::SeqCst)))
            .collect();
        top_prompts.sort_by_key(|x| std::cmp::Reverse(x.1));
        top_prompts.truncate(10);

        UsageReport {
            total_prompt_uses: total_uses,
            total_creates: self.create_count.load(Ordering::SeqCst),
            total_updates: self.update_count.load(Ordering::SeqCst),
            success_rate,
            total_tokens: self.total_tokens_consumed.load(Ordering::SeqCst),
            total_cost_usd: self.total_cost_micros.load(Ordering::SeqCst) as f64 / 1_000_000.0,
            unique_prompts_used: self.prompt_usage.len(),
            top_prompts,
        }
    }

    /// Get success rate as a percentage.
    pub fn success_rate(&self) -> f64 {
        let successes = self.success_count.load(Ordering::SeqCst);
        let failures = self.failure_count.load(Ordering::SeqCst);
        let total = successes + failures;
        if total > 0 {
            (successes as f64 * 100.0) / total as f64
        } else {
            0.0
        }
    }

    /// Get total cost in USD.
    pub fn total_cost_usd(&self) -> f64 {
        self.total_cost_micros.load(Ordering::SeqCst) as f64 / 1_000_000.0
    }

    /// Get adoption metrics.
    pub fn adoption_metrics(&self) -> AdoptionMetrics {
        let feature_usage: HashMap<String, u64> =
            self.daily_events
                .values()
                .flatten()
                .fold(HashMap::new(), |mut acc, e| {
                    let key = format!("{:?}", e.event_type);
                    *acc.entry(key).or_insert(0) += 1;
                    acc
                });

        AdoptionMetrics {
            active_users: self.active_users.len(),
            total_prompts_created: self.prompt_usage.len() as u64,
            prompts_per_user: if !self.active_users.is_empty() {
                self.prompt_usage.len() as f64 / self.active_users.len() as f64
            } else {
                0.0
            },
            feature_usage,
        }
    }

    /// Get cost trends as a real time-series bucketed by UTC day.
    ///
    /// Each returned [`CostTrend`] aggregates the cost, tokens, and request
    /// count of the events recorded on that day (the `period` is the
    /// `YYYY-MM-DD` bucket key). The series is sorted chronologically and
    /// spans as many buckets as the recorded events cover.
    pub fn cost_trends(&self) -> Vec<CostTrend> {
        let mut trends: Vec<CostTrend> = self
            .daily_events
            .iter()
            .map(|(period, events)| {
                let cost_micros: u64 = events.iter().map(|e| e.cost_micros).sum();
                let token_count: u64 = events.iter().map(|e| e.tokens_used).sum();
                CostTrend {
                    period: period.clone(),
                    cost_usd: cost_micros as f64 / 1_000_000.0,
                    token_count,
                    request_count: events.len() as u64,
                }
            })
            .collect();
        // Day-bucket keys are zero-padded `YYYY-MM-DD`, so lexical order is
        // chronological order.
        trends.sort_by(|a, b| a.period.cmp(&b.period));
        trends
    }

    /// Get total number of requests.
    pub fn total_requests(&self) -> u64 {
        self.success_count.load(Ordering::SeqCst) + self.failure_count.load(Ordering::SeqCst)
    }

    /// Reset all analytics data.
    pub fn reset(&mut self) {
        self.prompt_usage.clear();
        self.success_count.store(0, Ordering::SeqCst);
        self.failure_count.store(0, Ordering::SeqCst);
        self.create_count.store(0, Ordering::SeqCst);
        self.update_count.store(0, Ordering::SeqCst);
        self.total_tokens_consumed.store(0, Ordering::SeqCst);
        self.total_cost_micros.store(0, Ordering::SeqCst);
        self.active_users.clear();
        self.daily_events.clear();
        info!("Analytics data reset");
    }
}

impl Default for Analytics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event(event_type: EventType, prompt_id: &str, success: bool) -> AnalyticsEvent {
        sample_event_at(event_type, prompt_id, success, Utc::now())
    }

    /// Like [`sample_event`] but with an explicit timestamp, so tests can seed
    /// events into specific day buckets of the cost-trends series.
    fn sample_event_at(
        event_type: EventType,
        prompt_id: &str,
        success: bool,
        timestamp: DateTime<Utc>,
    ) -> AnalyticsEvent {
        AnalyticsEvent {
            event_type,
            prompt_id: prompt_id.to_string(),
            user_id: "user-1".to_string(),
            tokens_used: 100,
            cost_micros: 50_000,
            success,
            duration_ms: 200,
            timestamp,
        }
    }

    /// Build a fixed UTC instant at midnight on the given calendar day.
    fn day(year: i32, month: u32, dom: u32) -> DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(year, month, dom, 0, 0, 0).unwrap()
    }

    #[test]
    fn test_record_and_report() {
        let mut analytics = Analytics::new();
        analytics.record_event(sample_event(EventType::PromptUse, "p1", true));
        analytics.record_event(sample_event(EventType::PromptUse, "p1", true));
        analytics.record_event(sample_event(EventType::PromptUse, "p2", false));

        let report = analytics.usage_report();
        assert_eq!(report.total_prompt_uses, 3);
        assert_eq!(report.unique_prompts_used, 2);
    }

    #[test]
    fn test_success_rate() {
        let mut analytics = Analytics::new();
        analytics.record_event(sample_event(EventType::PromptUse, "p1", true));
        analytics.record_event(sample_event(EventType::PromptUse, "p2", true));
        analytics.record_event(sample_event(EventType::PromptUse, "p3", false));

        assert_eq!(analytics.success_rate(), 200.0 / 3.0);
    }

    #[test]
    fn test_total_cost() {
        let mut analytics = Analytics::new();
        analytics.record_event(sample_event(EventType::PromptUse, "p1", true));
        assert_eq!(analytics.total_cost_usd(), 0.05);
    }

    #[test]
    fn test_top_prompts() {
        let mut analytics = Analytics::new();
        for _ in 0..5 {
            analytics.record_event(sample_event(EventType::PromptUse, "popular", true));
        }
        analytics.record_event(sample_event(EventType::PromptUse, "rare", true));

        let report = analytics.usage_report();
        assert_eq!(report.top_prompts[0].0, "popular");
        assert_eq!(report.top_prompts[0].1, 5);
    }

    #[test]
    fn test_adoption_metrics() {
        let mut analytics = Analytics::new();
        analytics.record_event(sample_event(EventType::PromptUse, "p1", true));

        let adoption = analytics.adoption_metrics();
        assert_eq!(adoption.active_users, 1);
    }

    #[test]
    fn test_cost_trends() {
        let mut analytics = Analytics::new();
        analytics.record_event(sample_event(EventType::PromptUse, "p1", true));

        let trends = analytics.cost_trends();
        assert_eq!(trends.len(), 1);
        assert!(trends[0].cost_usd > 0.0);
    }

    #[test]
    fn test_cost_trends_multi_bucket_series() {
        let mut analytics = Analytics::new();
        // Two events on day 1, one on day 2, three on day 3 — three buckets.
        analytics.record_event(sample_event_at(
            EventType::PromptUse,
            "p1",
            true,
            day(2026, 1, 1).with_timezone(&Utc) + chrono::Duration::hours(2),
        ));
        analytics.record_event(sample_event_at(
            EventType::PromptUse,
            "p2",
            true,
            day(2026, 1, 1).with_timezone(&Utc) + chrono::Duration::hours(20),
        ));
        analytics.record_event(sample_event_at(
            EventType::PromptUse,
            "p3",
            true,
            day(2026, 1, 2),
        ));
        for _ in 0..3 {
            analytics.record_event(sample_event_at(
                EventType::PromptUse,
                "p4",
                true,
                day(2026, 1, 3),
            ));
        }

        let trends = analytics.cost_trends();
        // Three distinct day buckets, in chronological order.
        assert_eq!(trends.len(), 3);
        assert_eq!(trends[0].period, "2026-01-01");
        assert_eq!(trends[1].period, "2026-01-02");
        assert_eq!(trends[2].period, "2026-01-03");

        // Per-bucket request counts reflect how many events landed each day.
        assert_eq!(trends[0].request_count, 2);
        assert_eq!(trends[1].request_count, 1);
        assert_eq!(trends[2].request_count, 3);

        // Per-bucket cost/token aggregation (each event: 50_000 micros, 100 tokens).
        assert_eq!(trends[0].cost_usd, 0.10);
        assert_eq!(trends[0].token_count, 200);
        assert_eq!(trends[2].cost_usd, 0.15);
        assert_eq!(trends[2].token_count, 300);

        // The series totals reconcile with the flat aggregates.
        let series_cost: f64 = trends.iter().map(|t| t.cost_usd).sum();
        assert!((series_cost - analytics.total_cost_usd()).abs() < 1e-9);
        let series_requests: u64 = trends.iter().map(|t| t.request_count).sum();
        assert_eq!(series_requests, analytics.total_requests());
    }

    #[test]
    fn test_create_and_update_totals() {
        let mut analytics = Analytics::new();
        analytics.record_event(sample_event(EventType::PromptCreate, "p1", true));
        analytics.record_event(sample_event(EventType::PromptCreate, "p2", true));
        analytics.record_event(sample_event(EventType::PromptCreate, "p3", true));
        analytics.record_event(sample_event(EventType::PromptUpdate, "p1", true));
        analytics.record_event(sample_event(EventType::PromptUpdate, "p1", true));
        // Non-lifecycle events must not inflate the create/update totals.
        analytics.record_event(sample_event(EventType::PromptUse, "p1", true));
        analytics.record_event(sample_event(EventType::Search, "p1", true));

        let report = analytics.usage_report();
        assert_eq!(report.total_creates, 3);
        assert_eq!(report.total_updates, 2);
        assert_eq!(report.total_prompt_uses, 1);
    }

    #[test]
    fn test_create_update_totals_reset() {
        let mut analytics = Analytics::new();
        analytics.record_event(sample_event(EventType::PromptCreate, "p1", true));
        analytics.record_event(sample_event(EventType::PromptUpdate, "p1", true));
        assert_eq!(analytics.usage_report().total_creates, 1);
        assert_eq!(analytics.usage_report().total_updates, 1);

        analytics.reset();
        let report = analytics.usage_report();
        assert_eq!(report.total_creates, 0);
        assert_eq!(report.total_updates, 0);
        assert!(analytics.cost_trends().is_empty());
    }

    #[test]
    fn test_total_requests() {
        let mut analytics = Analytics::new();
        analytics.record_event(sample_event(EventType::PromptUse, "p1", true));
        analytics.record_event(sample_event(EventType::PromptUse, "p2", false));
        assert_eq!(analytics.total_requests(), 2);
    }

    #[test]
    fn test_reset() {
        let mut analytics = Analytics::new();
        analytics.record_event(sample_event(EventType::PromptUse, "p1", true));
        assert_eq!(analytics.total_requests(), 1);
        analytics.reset();
        assert_eq!(analytics.total_requests(), 0);
        assert_eq!(analytics.total_cost_usd(), 0.0);
    }

    #[test]
    fn test_default() {
        let analytics: Analytics = Default::default();
        assert_eq!(analytics.total_requests(), 0);
        assert_eq!(analytics.success_rate(), 0.0);
    }

    #[test]
    fn test_empty_report() {
        let analytics = Analytics::new();
        let report = analytics.usage_report();
        assert_eq!(report.total_prompt_uses, 0);
        assert_eq!(report.success_rate, 0.0);
        assert!(report.top_prompts.is_empty());
    }

    #[test]
    fn test_event_type_debug() {
        let e = EventType::PromptCreate;
        assert!(format!("{:?}", e).contains("PromptCreate"));
    }
}
