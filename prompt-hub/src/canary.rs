#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::*;
use tracing::{info, instrument, warn};
use uuid::Uuid;

/// Canary deployment engine for gradual feature rollouts
#[derive(Debug, Clone, Default)]
pub struct CanaryEngine;

impl CanaryEngine {
    /// Deploy a feature to a percentage of users
    #[instrument]
    pub async fn deploy(canary: &CanaryDeployment, user_id: Uuid) -> Result<bool> {
        // Check if user is in target list
        if canary.target_users.contains(&user_id) {
            info!(
                "User {} is in canary target list for feature '{}'",
                user_id, canary.feature
            );
            return Ok(true);
        }
        // Percentage-based rollout using hash of user_id + feature name
        let hash_input = format!("{}{}", user_id, canary.feature);
        let hash = sha2::Sha256::digest(hash_input.as_bytes());
        let user_bucket = (hash[0] as f64 / 255.0) * 100.0;
        let included = user_bucket < canary.canary_percentage;
        if included {
            info!(
                "User {} included in canary '{}' (bucket {:.1}% < {:.1}%)",
                user_id, canary.feature, user_bucket, canary.canary_percentage
            );
        }
        Ok(included)
    }

    /// Check if metrics indicate rollback is needed
    #[instrument]
    pub fn should_rollback(canary: &CanaryDeployment, error_rate: f64, latency_p99: f64) -> bool {
        if error_rate > canary.rollback_threshold {
            warn!(
                "Canary rollback triggered: error_rate {:.2} > threshold {:.2}",
                error_rate, canary.rollback_threshold
            );
            return true;
        }
        if latency_p99 > canary.rollback_threshold * 1000.0 {
            warn!(
                "Canary rollback triggered: latency_p99 {:.0}ms exceeds threshold",
                latency_p99
            );
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canary_target_user_always_included() {
        let uid = Uuid::new_v4();
        let canary = CanaryDeployment {
            feature: "test".to_string(),
            canary_percentage: 0.0,
            target_users: vec![uid],
            rollback_threshold: 0.05,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let included = rt.block_on(CanaryEngine::deploy(&canary, uid)).unwrap();
        assert!(included);
    }

    #[test]
    fn test_should_rollback() {
        let canary = CanaryDeployment {
            feature: "test".to_string(),
            canary_percentage: 10.0,
            target_users: vec![],
            rollback_threshold: 0.05,
        };
        assert!(CanaryEngine::should_rollback(&canary, 0.10, 100.0));
        assert!(!CanaryEngine::should_rollback(&canary, 0.01, 100.0));
    }
}
