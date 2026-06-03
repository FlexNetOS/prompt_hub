#![forbid(unsafe_code)]

use crate::error::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::time::{Duration, Instant};
use tracing::{info, instrument, warn};

/// Token quota enforcement with daily, hourly, and burst limits.
///
/// Tracks consumption against configurable quotas and resets
/// counters automatically when time boundaries are crossed.
#[derive(Debug)]
pub struct QuotaEnforcer {
    daily_limit: AtomicU64,
    hourly_limit: AtomicU64,
    burst_limit: AtomicU64,
    daily_used: AtomicU64,
    hourly_used: AtomicU64,
    burst_used: AtomicU64,
    hour_start: std::sync::RwLock<Instant>,
    day_start: std::sync::RwLock<Instant>,
}

/// Result of a quota check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaStatus {
    Allowed,
    DailyExceeded,
    HourlyExceeded,
    BurstExceeded,
}

/// Quota configuration for an organization or user.
#[derive(Debug, Clone)]
pub struct QuotaConfig {
    pub daily_limit: u64,
    pub hourly_limit: u64,
    pub burst_limit: u64,
}

impl QuotaEnforcer {
    /// Create a new quota enforcer with the given limits.
    pub fn new(daily_limit: u64, hourly_limit: u64, burst_limit: u64) -> Self {
        let now = Instant::now();
        Self {
            daily_limit: AtomicU64::new(daily_limit),
            hourly_limit: AtomicU64::new(hourly_limit),
            burst_limit: AtomicU64::new(burst_limit),
            daily_used: AtomicU64::new(0),
            hourly_used: AtomicU64::new(0),
            burst_used: AtomicU64::new(0),
            hour_start: std::sync::RwLock::new(now),
            day_start: std::sync::RwLock::new(now),
        }
    }

    /// Check and consume tokens if allowed.
    #[instrument(skip(self), fields(tokens))]
    pub fn check_and_consume(&self, tokens: u64) -> Result<QuotaStatus> {
        self.maybe_reset_windows();

        let daily_limit = self.daily_limit.load(Ordering::SeqCst);
        let hourly_limit = self.hourly_limit.load(Ordering::SeqCst);
        let burst_limit = self.burst_limit.load(Ordering::SeqCst);

        // Check burst first (most restrictive)
        let burst_current = self.burst_used.load(Ordering::SeqCst);
        if burst_current + tokens > burst_limit {
            warn!(
                "Burst quota exceeded: {}/{} (request: {})",
                burst_current, burst_limit, tokens
            );
            return Ok(QuotaStatus::BurstExceeded);
        }

        // Check hourly
        let hourly_current = self.hourly_used.load(Ordering::SeqCst);
        if hourly_current + tokens > hourly_limit {
            warn!("Hourly quota exceeded: {}/{}", hourly_current, hourly_limit);
            return Ok(QuotaStatus::HourlyExceeded);
        }

        // Check daily
        let daily_current = self.daily_used.load(Ordering::SeqCst);
        if daily_current + tokens > daily_limit {
            warn!("Daily quota exceeded: {}/{}", daily_current, daily_limit);
            return Ok(QuotaStatus::DailyExceeded);
        }

        // All checks passed - consume tokens
        self.burst_used.fetch_add(tokens, Ordering::SeqCst);
        self.hourly_used.fetch_add(tokens, Ordering::SeqCst);
        self.daily_used.fetch_add(tokens, Ordering::SeqCst);

        Ok(QuotaStatus::Allowed)
    }

    /// Get current usage statistics.
    pub fn usage(&self) -> QuotaUsage {
        self.maybe_reset_windows();
        QuotaUsage {
            daily_used: self.daily_used.load(Ordering::SeqCst),
            daily_limit: self.daily_limit.load(Ordering::SeqCst),
            hourly_used: self.hourly_used.load(Ordering::SeqCst),
            hourly_limit: self.hourly_limit.load(Ordering::SeqCst),
            burst_used: self.burst_used.load(Ordering::SeqCst),
            burst_limit: self.burst_limit.load(Ordering::SeqCst),
        }
    }

    /// Reset all counters (e.g., for testing or admin override).
    #[instrument(skip(self))]
    pub fn reset_all(&self) {
        self.daily_used.store(0, Ordering::SeqCst);
        self.hourly_used.store(0, Ordering::SeqCst);
        self.burst_used.store(0, Ordering::SeqCst);
        *self.hour_start.write().unwrap() = Instant::now();
        *self.day_start.write().unwrap() = Instant::now();
        info!("All quota counters reset");
    }

    /// Update quota configuration.
    pub fn configure(&self, config: &QuotaConfig) {
        self.daily_limit.store(config.daily_limit, Ordering::SeqCst);
        self.hourly_limit
            .store(config.hourly_limit, Ordering::SeqCst);
        self.burst_limit.store(config.burst_limit, Ordering::SeqCst);
    }

    /// Reset burst counter only (useful after a cooldown).
    pub fn reset_burst(&self) {
        self.burst_used.store(0, Ordering::SeqCst);
    }

    fn maybe_reset_windows(&self) {
        let now = Instant::now();

        // Check hourly window
        {
            let hour_start = self.hour_start.read().unwrap();
            if now.duration_since(*hour_start) >= Duration::from_secs(3600) {
                drop(hour_start);
                self.hourly_used.store(0, Ordering::SeqCst);
                self.burst_used.store(0, Ordering::SeqCst);
                *self.hour_start.write().unwrap() = now;
            }
        }

        // Check daily window
        {
            let day_start = self.day_start.read().unwrap();
            if now.duration_since(*day_start) >= Duration::from_secs(86400) {
                drop(day_start);
                self.daily_used.store(0, Ordering::SeqCst);
                self.hourly_used.store(0, Ordering::SeqCst);
                self.burst_used.store(0, Ordering::SeqCst);
                *self.day_start.write().unwrap() = now;
                *self.hour_start.write().unwrap() = now;
            }
        }
    }
}

/// Current quota usage snapshot.
#[derive(Debug, Clone)]
pub struct QuotaUsage {
    pub daily_used: u64,
    pub daily_limit: u64,
    pub hourly_used: u64,
    pub hourly_limit: u64,
    pub burst_used: u64,
    pub burst_limit: u64,
}

impl Default for QuotaEnforcer {
    fn default() -> Self {
        Self::new(1_000_000, 100_000, 10_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_allows_within_limits() {
        let q = QuotaEnforcer::new(1000, 500, 100);
        assert_eq!(q.check_and_consume(50).unwrap(), QuotaStatus::Allowed);
    }

    #[test]
    fn test_quota_blocks_burst_exceeded() {
        let q = QuotaEnforcer::new(1000, 500, 100);
        assert_eq!(
            q.check_and_consume(101).unwrap(),
            QuotaStatus::BurstExceeded
        );
    }

    #[test]
    fn test_quota_blocks_cumulative_burst() {
        let q = QuotaEnforcer::new(1000, 500, 100);
        assert_eq!(q.check_and_consume(50).unwrap(), QuotaStatus::Allowed);
        assert_eq!(q.check_and_consume(51).unwrap(), QuotaStatus::BurstExceeded);
    }

    #[test]
    fn test_quota_blocks_hourly_exceeded() {
        let q = QuotaEnforcer::new(1000, 100, 200);
        assert_eq!(
            q.check_and_consume(101).unwrap(),
            QuotaStatus::HourlyExceeded
        );
    }

    #[test]
    fn test_quota_blocks_daily_exceeded() {
        let q = QuotaEnforcer::new(50, 500, 200);
        assert_eq!(q.check_and_consume(51).unwrap(), QuotaStatus::DailyExceeded);
    }

    #[test]
    fn test_usage_snapshot() {
        let q = QuotaEnforcer::new(1000, 500, 100);
        q.check_and_consume(30).unwrap();
        let usage = q.usage();
        assert_eq!(usage.daily_used, 30);
        assert_eq!(usage.hourly_used, 30);
        assert_eq!(usage.burst_used, 30);
    }

    #[test]
    fn test_reset_all() {
        let q = QuotaEnforcer::new(1000, 500, 100);
        q.check_and_consume(50).unwrap();
        q.reset_all();
        let usage = q.usage();
        assert_eq!(usage.daily_used, 0);
        assert_eq!(usage.hourly_used, 0);
        assert_eq!(usage.burst_used, 0);
    }

    #[test]
    fn test_configure() {
        let q = QuotaEnforcer::new(1000, 500, 100);
        q.configure(&QuotaConfig {
            daily_limit: 2000,
            hourly_limit: 1000,
            burst_limit: 500,
        });
        let usage = q.usage();
        assert_eq!(usage.daily_limit, 2000);
        assert_eq!(usage.hourly_limit, 1000);
        assert_eq!(usage.burst_limit, 500);
    }

    #[test]
    fn test_reset_burst() {
        let q = QuotaEnforcer::new(1000, 500, 100);
        q.check_and_consume(80).unwrap();
        assert_eq!(q.usage().burst_used, 80);
        q.reset_burst();
        assert_eq!(q.usage().burst_used, 0);
        // Daily and hourly should still be tracked
        assert_eq!(q.usage().daily_used, 80);
    }

    #[test]
    fn test_default() {
        let q = QuotaEnforcer::default();
        assert_eq!(q.check_and_consume(1).unwrap(), QuotaStatus::Allowed);
    }
}
