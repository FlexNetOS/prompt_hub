#![forbid(unsafe_code)]

//! Gradual rollout engine for staged feature deployments.
//!
//! `RolloutEngine` is a stateless utility that replaces the former `CanaryEngine`.
//! It provides percentage-based user targeting via SHA-256 bucket hashing,
//! auto-rollback evaluation, and staged progression helpers.

use crate::models::*;
use sha2::{Digest, Sha256};
use tracing::{info, instrument, warn};
use uuid::Uuid;

/// Stateless rollout engine for gradual feature rollouts.
#[derive(Debug, Clone, Default)]
pub struct RolloutEngine;

impl RolloutEngine {
    /// Determine whether a user should see the new feature.
    ///
    /// First checks `target_users` (always included). Then hashes `user_id + feature`
    /// with SHA-256 to produce a bucket in `[0, 100)`; if the bucket is below the
    /// target percentage the user is included.
    #[instrument(skip(canary))]
    pub fn should_rollout(canary: &CanaryDeployment, user_id: Uuid) -> bool {
        // Check if user is in target list (always included regardless of percentage)
        if canary.target_users.contains(&user_id) {
            info!(
                "User {} is in target list for feature '{}'",
                user_id, canary.feature
            );
            return true;
        }
        // Percentage-based rollout using hash of user_id + feature name
        let hash_input = format!("{}{}", user_id, canary.feature);
        let hash = Sha256::digest(hash_input.as_bytes());
        let user_bucket = (hash[0] as f64 / 255.0) * 100.0;
        let included = user_bucket < canary.canary_percentage;
        if included {
            info!(
                "User {} included in rollout '{}' (bucket {:.1}% < {:.1}%)",
                user_id, canary.feature, user_bucket, canary.canary_percentage
            );
        }
        included
    }

    /// Evaluate whether metrics indicate a rollback is needed based on the
    /// policy configured in *config*.
    pub fn evaluate_rollback(
        config: &GraduatedRolloutConfig,
        error_rate: f64,
        latency_p99_ms: u64,
    ) -> bool {
        let should_rollback = match config.auto_rollback {
            AutoRollbackPolicy::OnErrorRate { threshold } => error_rate > threshold,
            AutoRollbackPolicy::OnLatencyP99 { sla_ms } => latency_p99_ms > sla_ms,
            AutoRollbackPolicy::OnBoth {
                error_rate: err_threshold,
                latency_p99_ms: lat_sla,
            } => error_rate > err_threshold || latency_p99_ms > lat_sla,
        };

        if should_rollback {
            warn!(
                "Rollout '{}' auto-rollback triggered (error_rate {:.4}, latency_p99 {}ms)",
                config.rollout_id, error_rate, latency_p99_ms
            );
        }
        should_rollback
    }

    /// Advance *segment* to the next rollout stage. Returns the new stage if changed,
    /// or `None` if already at Production.
    pub fn advance_stage(segment: &mut RolloutSegment) -> Option<RolloutStage> {
        let next = match segment.rollout_stage {
            RolloutStage::Internal => RolloutStage::Alpha(10),
            RolloutStage::Alpha(p) => {
                if p >= 50 {
                    return None; // already past alpha
                }
                let next_p = std::cmp::min(p * 2, 50);
                RolloutStage::Alpha(next_p)
            }
            RolloutStage::Beta50(_v) => RolloutStage::Beta90(5),
            RolloutStage::Beta90(_v) => RolloutStage::Production,
            RolloutStage::Production => return None,
        };
        segment.rollout_stage = next.clone();
        info!(
            "Segment '{}' advanced to {:?}",
            segment.name, segment.rollout_stage
        );
        Some(next)
    }

    /// Create a new rollout configuration with the given parameters.
    #[instrument(skip(segments))]
    pub fn create_config(
        rollout_id: &str,
        feature: &str,
        segments: &[RolloutSegment],
    ) -> GraduatedRolloutConfig {
        GraduatedRolloutConfig {
            rollout_id: rollout_id.to_string(),
            feature: feature.to_string(),
            segments: segments.to_vec(),
            auto_rollback: AutoRollbackPolicy::OnErrorRate { threshold: 0.05 },
            active: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_target_user_always_included() {
        let uid = Uuid::new_v4();
        let canary = CanaryDeployment {
            feature: "test".to_string(),
            canary_percentage: 0.0,
            target_users: vec![uid],
            rollback_threshold: 0.05,
        };
        assert!(RolloutEngine::should_rollout(&canary, uid));
    }

    #[test]
    fn test_hash_bucket_zero_pct_excludes_non_target() {
        let uid = Uuid::new_v4();
        let canary = CanaryDeployment {
            feature: "test".to_string(),
            canary_percentage: 0.0,
            target_users: vec![],
            rollback_threshold: 0.05,
        };
        assert!(!RolloutEngine::should_rollout(&canary, uid));
    }

    #[test]
    fn test_rollback_error_rate() {
        let config = GraduatedRolloutConfig {
            rollout_id: "r1".into(),
            feature: "f1".into(),
            segments: vec![],
            auto_rollback: AutoRollbackPolicy::OnErrorRate { threshold: 0.05 },
            active: true,
        };
        assert!(RolloutEngine::evaluate_rollback(&config, 0.10, 200));
        assert!(!RolloutEngine::evaluate_rollback(&config, 0.01, 200));
    }

    #[test]
    fn test_rollback_latency_p99() {
        let config = GraduatedRolloutConfig {
            rollout_id: "r2".into(),
            feature: "f2".into(),
            segments: vec![],
            auto_rollback: AutoRollbackPolicy::OnLatencyP99 { sla_ms: 500 },
            active: true,
        };
        assert!(RolloutEngine::evaluate_rollback(&config, 0.01, 600));
        assert!(!RolloutEngine::evaluate_rollback(&config, 0.01, 400));
    }

    #[test]
    fn test_rollback_both_conditions() {
        let config = GraduatedRolloutConfig {
            rollout_id: "r3".into(),
            feature: "f3".into(),
            segments: vec![],
            auto_rollback: AutoRollbackPolicy::OnBoth {
                error_rate: 0.10,
                latency_p99_ms: 500,
            },
            active: true,
        };
        // Both exceed thresholds => rollback
        assert!(RolloutEngine::evaluate_rollback(&config, 0.20, 600));
        // Only error exceeds => rollback (any single triggers)
        assert!(RolloutEngine::evaluate_rollback(&config, 0.20, 400));
        // Neither exceeds => no rollback
        assert!(!RolloutEngine::evaluate_rollback(&config, 0.05, 400));
    }

    #[test]
    fn test_advance_stages() {
        let mut seg = RolloutSegment {
            name: "alpha".into(),
            percentage: 10,
            target_users: vec![],
            rollout_stage: RolloutStage::Internal,
            created_at: Utc::now(),
        };
        // Internal -> Alpha(10)
        assert!(matches!(
            RolloutEngine::advance_stage(&mut seg),
            Some(RolloutStage::Alpha(10))
        ));
        // Alpha(10) -> Alpha(20) (doubling)
        assert!(matches!(
            RolloutEngine::advance_stage(&mut seg),
            Some(RolloutStage::Alpha(20))
        ));
        // Alpha(20) -> Alpha(40) (doubling)
        assert!(matches!(
            RolloutEngine::advance_stage(&mut seg),
            Some(RolloutStage::Alpha(40))
        ));
        // Alpha(40) -> Alpha(50) (min(p*2, 50) = min(80, 50) = 50)
        assert!(matches!(
            RolloutEngine::advance_stage(&mut seg),
            Some(RolloutStage::Alpha(50))
        ));
        // Alpha(50) -> None (guard: p >= 50 blocks further auto-advancement)
        assert!(RolloutEngine::advance_stage(&mut seg).is_none());
        // Manual jump to Beta90, then Production
        seg.rollout_stage = RolloutStage::Beta90(5);
        assert!(matches!(
            RolloutEngine::advance_stage(&mut seg),
            Some(RolloutStage::Production)
        ));
        // Production -> None (terminal)
        assert!(RolloutEngine::advance_stage(&mut seg).is_none());
    }
}
