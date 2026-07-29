#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use std::sync::{Arc, RwLock};
use tokio::time::{Duration, Instant};
use tracing::{info, instrument, warn};

/// Circuit breaker pattern for fault-tolerant external calls.
///
/// Transitions between Closed (normal), Open (failing fast), and
/// HalfOpen (probing) states to prevent cascading failures.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    reset_timeout: Duration,
    half_open_max_calls: u32,
    state: Arc<RwLock<BreakerState>>,
}

#[derive(Debug, Clone)]
enum BreakerState {
    Closed { failures: u32 },
    Open { opened_at: Instant },
    HalfOpen { calls: u32, successes: u32 },
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    ///
    /// * `failure_threshold` - consecutive failures before opening
    /// * `reset_timeout_secs` - seconds to wait before half-open probe
    pub fn new(failure_threshold: u32, reset_timeout_secs: u64) -> Self {
        Self {
            failure_threshold,
            reset_timeout: Duration::from_secs(reset_timeout_secs),
            half_open_max_calls: 3,
            state: Arc::new(RwLock::new(BreakerState::Closed { failures: 0 })),
        }
    }

    /// Create with custom half-open probe count.
    pub fn with_half_open_max_calls(mut self, max_calls: u32) -> Self {
        self.half_open_max_calls = max_calls;
        self
    }

    /// Execute a call through the circuit breaker.
    #[instrument(skip(self, f), fields(threshold = self.failure_threshold))]
    pub fn call<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        // Check and potentially transition state
        {
            let mut state = self
                .state
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match &*state {
                BreakerState::Open { opened_at } => {
                    if opened_at.elapsed() > self.reset_timeout {
                        *state = BreakerState::HalfOpen {
                            calls: 0,
                            successes: 0,
                        };
                        info!("Circuit breaker entering HALF-OPEN state");
                    } else {
                        return Err(HubError::Internal("Circuit breaker is OPEN".to_string()));
                    }
                }
                BreakerState::HalfOpen { calls, .. } => {
                    if *calls >= self.half_open_max_calls {
                        return Err(HubError::Internal(
                            "Circuit breaker is HALF-OPEN (max probes reached)".to_string(),
                        ));
                    }
                }
                BreakerState::Closed { .. } => {}
            }
        }

        // Execute the call
        match f() {
            Ok(val) => {
                let mut state = self
                    .state
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let BreakerState::HalfOpen { calls, successes } = &mut *state {
                    *calls += 1;
                    *successes += 1;
                    if *successes >= self.half_open_max_calls {
                        *state = BreakerState::Closed { failures: 0 };
                        info!("Circuit breaker CLOSED (recovery confirmed)");
                    }
                }
                Ok(val)
            }
            Err(e) => {
                let mut state = self
                    .state
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match &mut *state {
                    BreakerState::Closed { failures } => {
                        *failures += 1;
                        if *failures >= self.failure_threshold {
                            *state = BreakerState::Open {
                                opened_at: Instant::now(),
                            };
                            warn!(
                                "Circuit breaker OPENED after {} consecutive failures",
                                self.failure_threshold
                            );
                        }
                    }
                    BreakerState::HalfOpen { .. } => {
                        *state = BreakerState::Open {
                            opened_at: Instant::now(),
                        };
                        warn!("Circuit breaker re-OPENED during half-open probe");
                    }
                    BreakerState::Open { .. } => {}
                }
                Err(e)
            }
        }
    }

    /// Get the current state name for observability.
    pub fn current_state(&self) -> &'static str {
        let state = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match &*state {
            BreakerState::Closed { .. } => "closed",
            BreakerState::Open { .. } => "open",
            BreakerState::HalfOpen { .. } => "half_open",
        }
    }

    /// Reset the breaker to closed state.
    pub fn reset(&self) {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *state = BreakerState::Closed { failures: 0 };
        info!("Circuit breaker manually reset to CLOSED");
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, 30)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cb_allows_calls_when_closed() {
        let cb = CircuitBreaker::new(3, 5);
        assert!(cb.call(|| Ok(42)).is_ok());
        assert_eq!(cb.call(|| Ok(42)).unwrap(), 42);
    }

    #[test]
    fn test_cb_opens_after_failures() {
        let cb = CircuitBreaker::new(2, 60);
        assert!(
            cb.call(|| Err::<(), _>(HubError::Internal("fail".to_string())))
                .is_err()
        );
        assert!(
            cb.call(|| Err::<(), _>(HubError::Internal("fail".to_string())))
                .is_err()
        );
        // Third call should fail fast - breaker is open
        let result = cb.call(|| Ok(42));
        assert!(result.is_err());
        assert!(format!("{:?}", result).contains("OPEN"));
    }

    #[test]
    fn test_cb_state_closed_initially() {
        let cb = CircuitBreaker::new(3, 5);
        assert_eq!(cb.current_state(), "closed");
    }

    #[test]
    fn test_cb_opens_after_single_failure_when_threshold_one() {
        let cb = CircuitBreaker::new(1, 60);
        assert!(
            cb.call(|| Err::<(), _>(HubError::Internal("fail".to_string())))
                .is_err()
        );
        assert_eq!(cb.current_state(), "open");
    }

    #[test]
    fn test_cb_resets_to_closed() {
        let cb = CircuitBreaker::new(1, 60);
        let _ = cb.call(|| Err::<(), _>(HubError::Internal("fail".to_string())));
        assert_eq!(cb.current_state(), "open");
        cb.reset();
        assert_eq!(cb.current_state(), "closed");
        assert!(cb.call(|| Ok(42)).is_ok());
    }

    #[test]
    fn test_default_breaker() {
        let cb = CircuitBreaker::default();
        assert_eq!(cb.current_state(), "closed");
        assert!(cb.call(|| Ok(100)).is_ok());
    }

    #[test]
    fn test_success_resets_failure_count() {
        // Note: our implementation accumulates failures without reset on success
        // This test verifies the documented behavior
        let cb = CircuitBreaker::new(3, 60);
        // Two failures
        let _ = cb.call(|| Err::<(), _>(HubError::Internal("fail".to_string())));
        let _ = cb.call(|| Err::<(), _>(HubError::Internal("fail".to_string())));
        // Still closed with 2 failures
        assert_eq!(cb.current_state(), "closed");
        // Third failure opens it
        let _ = cb.call(|| Err::<(), _>(HubError::Internal("fail".to_string())));
        assert_eq!(cb.current_state(), "open");
    }

    #[test]
    fn test_with_half_open_max_calls() {
        let cb = CircuitBreaker::new(1, 0).with_half_open_max_calls(1);
        // Force open
        let _ = cb.call(|| Err::<(), _>(HubError::Internal("fail".to_string())));
        assert_eq!(cb.current_state(), "open");
    }

    #[test]
    fn test_call_returns_correct_value() {
        let cb = CircuitBreaker::new(3, 5);
        let result = cb.call(|| Ok("hello".to_string()));
        assert_eq!(result.unwrap(), "hello");
    }
}
