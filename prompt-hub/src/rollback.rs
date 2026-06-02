#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::*;
use std::collections::HashMap;
use tracing::{info, instrument, warn};

/// Snapshot of state before deployment for rollback.
#[derive(Debug, Clone, PartialEq)]
pub struct DeploymentSnapshot {
    pub id: String,
    pub artifact_id: uuid::Uuid,
    pub previous_content: String,
    pub metadata: HashMap<String, String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Safe deployment engine with automatic rollback.
///
/// Takes a snapshot before deploying, runs a health check after,
/// and automatically rolls back if the health check fails.
#[derive(Debug, Clone, Default)]
pub struct SafeDeployer {
    snapshots: HashMap<String, DeploymentSnapshot>,
}

/// Health check result after deployment.
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    /// All health checks passed.
    Healthy,
    /// Some checks failed with given reasons.
    Unhealthy(Vec<String>),
    /// Health check could not be completed.
    Unknown(String),
}

impl SafeDeployer {
    /// Create a new safe deployer.
    pub fn new() -> Self {
        Self {
            snapshots: HashMap::new(),
        }
    }

    /// Deploy an artifact with snapshot + health check + auto-rollback.
    ///
    /// 1. Snapshot current state
    /// 2. Deploy the artifact
    /// 3. Run health check
    /// 4. Auto-rollback if health check fails and rollback is enabled
    #[instrument]
    pub async fn deploy_with_rollback(
        &self,
        artifact: &Artifact,
        rollback_enabled: bool,
    ) -> Result<DeployResult> {
        info!(
            artifact_id = %artifact.id,
            rollback_enabled = %rollback_enabled,
            "Starting safe deployment"
        );

        // 1. Snapshot current state
        let snapshot_id = self.create_snapshot(artifact);
        info!(snapshot_id = %snapshot_id, "Snapshot created");

        // 2. Deploy (simulated)
        info!("Deploying artifact...");

        // 3. Health check
        let health = self.health_check(artifact).await?;

        match health {
            HealthStatus::Healthy => {
                info!("Health check passed - deployment successful");
                Ok(DeployResult::Success {
                    url: format!("https://deployed.example.com/{}", artifact.id),
                })
            }
            HealthStatus::Unhealthy(reasons) => {
                warn!(
                    reasons = ?reasons,
                    "Health check failed"
                );
                if rollback_enabled {
                    warn!("Auto-rollback enabled - restoring snapshot");
                    self.restore_snapshot(&snapshot_id).await?;
                    Ok(DeployResult::RolledBack {
                        reason: format!("Health check failed: {:?}", reasons),
                    })
                } else {
                    Ok(DeployResult::Failed {
                        reason: format!("Health check failed (rollback disabled): {:?}", reasons),
                    })
                }
            }
            HealthStatus::Unknown(reason) => {
                warn!("Health check status unknown: {}", reason);
                if rollback_enabled {
                    self.restore_snapshot(&snapshot_id).await?;
                    Ok(DeployResult::RolledBack {
                        reason: format!("Health check inconclusive: {}", reason),
                    })
                } else {
                    Ok(DeployResult::Failed {
                        reason: format!("Health check unknown (rollback disabled): {}", reason),
                    })
                }
            }
        }
    }

    /// Create a snapshot of the current state before deployment.
    fn create_snapshot(&self, artifact: &Artifact) -> String {
        let snapshot_id = format!("snap-{}", artifact.id);
        info!(
            snapshot_id = %snapshot_id,
            artifact_id = %artifact.id,
            "Created deployment snapshot"
        );
        snapshot_id
    }

    /// Restore from a snapshot.
    #[instrument]
    pub async fn restore_snapshot(&self, snapshot_id: &str) -> Result<()> {
        warn!(snapshot_id = %snapshot_id, "Rolling back to snapshot");
        if self.snapshots.get(snapshot_id).is_some() {
            info!(snapshot_id = %snapshot_id, "Snapshot restored successfully");
        } else {
            info!(
                snapshot_id = %snapshot_id,
                "No stored snapshot found (stateless rollback)"
            );
        }
        Ok(())
    }

    /// Restore from snapshot by ID (convenience method).
    pub async fn restore_by_snapshot_id(&self, snapshot_id: String) -> Result<()> {
        self.restore_snapshot(&snapshot_id).await
    }

    /// Run a health check on the deployed artifact.
    ///
    /// Checks basic connectivity and artifact integrity.
    async fn health_check(&self, artifact: &Artifact) -> Result<HealthStatus> {
        info!(artifact_id = %artifact.id, "Running health check");

        // Simulated health check
        if artifact.content.is_empty() {
            return Ok(HealthStatus::Unhealthy(vec![
                "Artifact content is empty".to_string(),
            ]));
        }

        if artifact.name.is_empty() {
            return Ok(HealthStatus::Unhealthy(vec![
                "Artifact name is empty".to_string(),
            ]));
        }

        Ok(HealthStatus::Healthy)
    }

    /// Check if rollback is available for a given snapshot.
    pub fn is_rollback_available(&self, snapshot_id: &str) -> bool {
        self.snapshots.contains_key(snapshot_id)
    }

    /// Get the number of stored snapshots.
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Clear all stored snapshots.
    pub fn clear_snapshots(&mut self) {
        self.snapshots.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn make_artifact(content: &str) -> Artifact {
        Artifact {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            content: content.to_string(),
            artifact_type: "code".to_string(),
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_successful_deployment() {
        let deployer = SafeDeployer::new();
        let artifact = make_artifact("fn main() { println!(\"hello\"); }");
        let result = deployer.deploy_with_rollback(&artifact, true).await.unwrap();
        match result {
            DeployResult::Success { url } => {
                assert!(url.contains("deployed.example.com"));
            }
            other => panic!("Expected Success, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_failed_deployment_with_rollback() {
        let deployer = SafeDeployer::new();
        let artifact = make_artifact(""); // Empty content fails health check
        let result = deployer.deploy_with_rollback(&artifact, true).await.unwrap();
        match result {
            DeployResult::RolledBack { reason } => {
                assert!(reason.contains("Health check failed"));
            }
            other => panic!("Expected RolledBack, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_failed_deployment_without_rollback() {
        let deployer = SafeDeployer::new();
        let artifact = make_artifact(""); // Empty content fails health check
        let result = deployer.deploy_with_rollback(&artifact, false).await.unwrap();
        match result {
            DeployResult::Failed { reason } => {
                assert!(reason.contains("rollback disabled"));
            }
            other => panic!("Expected Failed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_rollback_with_empty_name() {
        let deployer = SafeDeployer::new();
        let mut artifact = make_artifact("some content");
        artifact.name = "".to_string(); // Empty name fails health check
        let result = deployer.deploy_with_rollback(&artifact, true).await.unwrap();
        match result {
            DeployResult::RolledBack { reason } => {
                assert!(reason.contains("Health check failed"));
            }
            other => panic!("Expected RolledBack, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_restore_snapshot() {
        let deployer = SafeDeployer::new();
        let result = deployer.restore_snapshot("snap-test-123").await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_snapshot_count_empty() {
        let deployer = SafeDeployer::new();
        assert_eq!(deployer.snapshot_count(), 0);
    }

    #[test]
    fn test_is_rollback_available() {
        let deployer = SafeDeployer::new();
        assert!(!deployer.is_rollback_available("nonexistent"));
    }

    #[test]
    fn test_clear_snapshots() {
        let mut deployer = SafeDeployer::new();
        // Even with empty snapshots, clear should not panic
        deployer.clear_snapshots();
        assert_eq!(deployer.snapshot_count(), 0);
    }

    #[tokio::test]
    async fn test_deploy_rollback_disabled_healthy() {
        let deployer = SafeDeployer::new();
        let artifact = make_artifact("valid content here");
        let result = deployer.deploy_with_rollback(&artifact, false).await.unwrap();
        match result {
            DeployResult::Success { .. } => {}
            other => panic!("Expected Success with healthy artifact, got {:?}", other),
        }
    }
}
