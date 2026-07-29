#![forbid(unsafe_code)]

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use tracing::{info, instrument, warn};

/// User satisfaction tracking system.
///
/// Collects CSAT/NPS scores, tracks the success funnel, and calculates
/// one-shot success rate with trend analysis.
#[derive(Debug)]
pub struct SatisfactionTracker {
    ratings: Arc<RwLock<VecDeque<RatingEntry>>>,
    success_events: Arc<RwLock<VecDeque<SuccessEvent>>>,
    max_history: usize,
}

/// A single satisfaction rating.
#[derive(Debug, Clone)]
pub struct RatingEntry {
    pub score: u8,
    pub kind: RatingKind,
    pub context: String,
    pub timestamp: std::time::Instant,
}

/// Type of satisfaction rating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatingKind {
    /// Customer Satisfaction Score (1-5)
    CSAT,
    /// Net Promoter Score (1-10)
    NPS,
    /// Custom score (1-5)
    Custom,
}

/// A success/failure event in the funnel.
#[derive(Debug, Clone)]
pub struct SuccessEvent {
    pub prompt_id: String,
    pub successful: bool,
    pub attempts: u8,
    pub timestamp: std::time::Instant,
}

/// Satisfaction metrics summary.
#[derive(Debug, Clone, Serialize)]
pub struct SatisfactionMetrics {
    pub csat_average: f64,
    pub nps_score: f64,
    pub one_shot_success_rate: f64,
    pub total_ratings: usize,
    pub total_events: usize,
    pub recent_trend: TrendDirection,
}

/// Trend direction for satisfaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TrendDirection {
    Improving,
    Stable,
    Declining,
}

impl SatisfactionTracker {
    /// Create a new satisfaction tracker.
    pub fn new(max_history: usize) -> Self {
        Self {
            ratings: Arc::new(RwLock::new(VecDeque::new())),
            success_events: Arc::new(RwLock::new(VecDeque::new())),
            max_history,
        }
    }

    /// Record a CSAT rating (1-5).
    #[instrument(skip(self), fields(score))]
    pub fn record_csat(&self, score: u8, context: &str) {
        if !(1..=5).contains(&score) {
            warn!("Invalid CSAT score: {}, must be 1-5", score);
            return;
        }
        let mut ratings = self
            .ratings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ratings.push_back(RatingEntry {
            score,
            kind: RatingKind::CSAT,
            context: context.to_string(),
            timestamp: std::time::Instant::now(),
        });
        if ratings.len() > self.max_history {
            ratings.pop_front();
        }
        info!("Recorded CSAT score: {}/5", score);
    }

    /// Record an NPS rating (1-10).
    #[instrument(skip(self), fields(score))]
    pub fn record_nps(&self, score: u8) {
        if !(1..=10).contains(&score) {
            warn!("Invalid NPS score: {}, must be 1-10", score);
            return;
        }
        let mut ratings = self
            .ratings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ratings.push_back(RatingEntry {
            score,
            kind: RatingKind::NPS,
            context: "nps".to_string(),
            timestamp: std::time::Instant::now(),
        });
        if ratings.len() > self.max_history {
            ratings.pop_front();
        }
        info!("Recorded NPS score: {}/10", score);
    }

    /// Record a success event for the funnel.
    #[instrument(skip(self), fields(successful, attempts))]
    pub fn record_event(&self, prompt_id: &str, successful: bool, attempts: u8) {
        let mut events = self
            .success_events
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        events.push_back(SuccessEvent {
            prompt_id: prompt_id.to_string(),
            successful,
            attempts,
            timestamp: std::time::Instant::now(),
        });
        if events.len() > self.max_history {
            events.pop_front();
        }
    }

    /// Calculate one-shot success rate (succeeded on first attempt).
    pub fn one_shot_success_rate(&self) -> f64 {
        let events = self
            .success_events
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let one_shot: usize = events
            .iter()
            .filter(|e| e.successful && e.attempts == 1)
            .count();
        let total_successful: usize = events.iter().filter(|e| e.successful).count();
        if total_successful == 0 {
            0.0
        } else {
            (one_shot as f64 / total_successful as f64) * 100.0
        }
    }

    /// Compute overall satisfaction metrics.
    pub fn metrics(&self) -> SatisfactionMetrics {
        let ratings = self
            .ratings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let events = self
            .success_events
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let csat_scores: Vec<u8> = ratings
            .iter()
            .filter(|r| r.kind == RatingKind::CSAT)
            .map(|r| r.score)
            .collect();
        let csat_avg = if csat_scores.is_empty() {
            0.0
        } else {
            csat_scores.iter().map(|&s| s as f64).sum::<f64>() / csat_scores.len() as f64
        };

        let nps_scores: Vec<u8> = ratings
            .iter()
            .filter(|r| r.kind == RatingKind::NPS)
            .map(|r| r.score)
            .collect();
        let nps = if nps_scores.is_empty() {
            0.0
        } else {
            let promoters = nps_scores.iter().filter(|&&s| s >= 9).count() as f64;
            let detractors = nps_scores.iter().filter(|&&s| s <= 6).count() as f64;
            let total = nps_scores.len() as f64;
            if total > 0.0 {
                ((promoters - detractors) / total) * 100.0
            } else {
                0.0
            }
        };

        let recent_trend = self.calculate_trend(&ratings);

        SatisfactionMetrics {
            csat_average: csat_avg,
            nps_score: nps,
            one_shot_success_rate: self.one_shot_success_rate(),
            total_ratings: ratings.len(),
            total_events: events.len(),
            recent_trend,
        }
    }

    fn calculate_trend(&self, ratings: &VecDeque<RatingEntry>) -> TrendDirection {
        if ratings.len() < 4 {
            return TrendDirection::Stable;
        }

        let half = ratings.len() / 2;
        let recent: Vec<u8> = ratings.iter().rev().take(half).map(|r| r.score).collect();
        let older: Vec<u8> = ratings.iter().take(half).map(|r| r.score).collect();

        let recent_avg = recent.iter().sum::<u8>() as f64 / recent.len().max(1) as f64;
        let older_avg = older.iter().sum::<u8>() as f64 / older.len().max(1) as f64;

        if recent_avg > older_avg + 0.5 {
            TrendDirection::Improving
        } else if recent_avg < older_avg - 0.5 {
            TrendDirection::Declining
        } else {
            TrendDirection::Stable
        }
    }

    /// Get the total number of ratings recorded.
    pub fn rating_count(&self) -> usize {
        self.ratings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Get the total number of events recorded.
    pub fn event_count(&self) -> usize {
        self.success_events
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Clear all data.
    pub fn clear(&self) {
        self.ratings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.success_events
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        info!("Satisfaction tracker cleared");
    }
}

impl Default for SatisfactionTracker {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_csat() {
        let tracker = SatisfactionTracker::new(100);
        tracker.record_csat(5, "Great experience");
        assert_eq!(tracker.rating_count(), 1);
    }

    #[test]
    fn test_record_csat_invalid() {
        let tracker = SatisfactionTracker::new(100);
        tracker.record_csat(0, "test");
        assert_eq!(tracker.rating_count(), 0);
        tracker.record_csat(6, "test");
        assert_eq!(tracker.rating_count(), 0);
    }

    #[test]
    fn test_record_nps() {
        let tracker = SatisfactionTracker::new(100);
        tracker.record_nps(9);
        assert_eq!(tracker.rating_count(), 1);
    }

    #[test]
    fn test_record_nps_invalid() {
        let tracker = SatisfactionTracker::new(100);
        tracker.record_nps(0);
        assert_eq!(tracker.rating_count(), 0);
        tracker.record_nps(11);
        assert_eq!(tracker.rating_count(), 0);
    }

    #[test]
    fn test_one_shot_success_rate() {
        let tracker = SatisfactionTracker::new(100);
        tracker.record_event("p1", true, 1); // one-shot success
        tracker.record_event("p2", true, 2); // took 2 attempts
        tracker.record_event("p3", false, 1); // failed
        assert_eq!(tracker.one_shot_success_rate(), 50.0);
    }

    #[test]
    fn test_csat_average() {
        let tracker = SatisfactionTracker::new(100);
        tracker.record_csat(3, "ok");
        tracker.record_csat(5, "great");
        let metrics = tracker.metrics();
        assert_eq!(metrics.csat_average, 4.0);
    }

    #[test]
    fn test_nps_calculation() {
        let tracker = SatisfactionTracker::new(100);
        tracker.record_nps(10); // promoter
        tracker.record_nps(9); // promoter
        tracker.record_nps(5); // detractor
        let metrics = tracker.metrics();
        // (2 - 1) / 3 * 100 = 33.33...
        assert!(
            (metrics.nps_score - 33.33).abs() < 0.1,
            "NPS score: {}",
            metrics.nps_score
        );
    }

    #[test]
    fn test_trend_improving() {
        let tracker = SatisfactionTracker::new(100);
        // Old low scores
        for _ in 0..5 {
            tracker.record_csat(2, "old");
        }
        // New high scores
        for _ in 0..5 {
            tracker.record_csat(5, "new");
        }
        let metrics = tracker.metrics();
        assert_eq!(metrics.recent_trend, TrendDirection::Improving);
    }

    #[test]
    fn test_history_limit() {
        let tracker = SatisfactionTracker::new(5);
        for i in 1..=10 {
            tracker.record_csat(3, &format!("test{}", i));
        }
        assert_eq!(tracker.rating_count(), 5);
    }

    #[test]
    fn test_clear() {
        let tracker = SatisfactionTracker::new(100);
        tracker.record_csat(5, "great");
        tracker.record_event("p1", true, 1);
        tracker.clear();
        assert_eq!(tracker.rating_count(), 0);
        assert_eq!(tracker.event_count(), 0);
    }

    #[test]
    fn test_default() {
        let tracker: SatisfactionTracker = Default::default();
        assert_eq!(tracker.rating_count(), 0);
        assert_eq!(tracker.event_count(), 0);
    }

    #[test]
    fn test_empty_metrics() {
        let tracker = SatisfactionTracker::new(100);
        let metrics = tracker.metrics();
        assert_eq!(metrics.csat_average, 0.0);
        assert_eq!(metrics.nps_score, 0.0);
        assert_eq!(metrics.one_shot_success_rate, 0.0);
        assert_eq!(metrics.recent_trend, TrendDirection::Stable);
    }

    #[test]
    fn test_event_count() {
        let tracker = SatisfactionTracker::new(100);
        tracker.record_event("p1", true, 1);
        tracker.record_event("p2", false, 3);
        assert_eq!(tracker.event_count(), 2);
    }

    #[test]
    fn test_trend_stable() {
        let tracker = SatisfactionTracker::new(100);
        for _ in 0..4 {
            tracker.record_csat(3, "same");
        }
        let metrics = tracker.metrics();
        assert_eq!(metrics.recent_trend, TrendDirection::Stable);
    }
}
