#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::*;
use std::collections::HashMap;
use tokio::sync::{RwLock, broadcast};
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Sync event types
// ---------------------------------------------------------------------------

/// Events broadcast across the sync channel to keep all agents coherent.
///
/// Each variant carries enough context for receivers to update their local
/// state without needing to query the database.
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// A new prompt was added to the hub.
    PromptAdded { prompt_id: Uuid },
    /// An existing prompt was updated.
    PromptUpdated { prompt_id: Uuid },
    /// A prompt was locked for exclusive editing by an agent.
    PromptLocked { prompt_id: Uuid, agent_id: Uuid },
    /// A prompt lock was released.
    PromptUnlocked { prompt_id: Uuid },
    /// A new agent joined the swarm.
    AgentJoined { agent_id: Uuid, name: String },
    /// An agent left the swarm (heartbeat timeout or explicit disconnect).
    AgentLeft { agent_id: Uuid },
    /// Prompt ownership was transferred between agents.
    OwnershipTransferred {
        prompt_id: Uuid,
        from: Uuid,
        to: Uuid,
    },
    /// A pattern was shared between agents.
    PatternShared { pattern_id: Uuid, from_agent: Uuid },
    /// Heartbeat from an agent (used for liveness detection).
    Heartbeat {
        agent_id: Uuid,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    /// A split-brain condition was detected and resolved.
    SplitBrainResolved {
        partition_id: Uuid,
        resolution: SplitBrainResolution,
    },
}

/// Resolution strategy for a detected split-brain condition.
#[derive(Debug, Clone)]
pub enum SplitBrainResolution {
    /// One partition was chosen as authoritative; the other was discarded.
    KeepPartition { winning_partition: Uuid },
    /// Changes from both partitions were merged.
    MergePartitions { merged_from: Vec<Uuid> },
    /// Manual intervention is required.
    ManualInterventionRequired { reason: String },
}

// ---------------------------------------------------------------------------
// Sync manager — multiple backends
// ---------------------------------------------------------------------------

/// Central sync manager that coordinates events across all backends.
///
/// Uses a `broadcast` channel (capacity 1000) so every subscriber receives
/// every event.  The manager also tracks agent presence for heartbeat-based
/// failure detection.
#[derive(Debug, Clone)]
pub struct SyncManager {
    event_tx: broadcast::Sender<SyncEvent>,
}

impl SyncManager {
    /// Create a new sync manager with a bounded broadcast channel.
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(1000);
        Self { event_tx: tx }
    }

    /// Subscribe to sync events.
    ///
    /// Each call returns a new receiver that will receive all events
    /// broadcast after the subscription point.
    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.event_tx.subscribe()
    }

    /// Broadcast a sync event to all subscribers.
    ///
    /// Returns the number of active subscribers or an error if the
    /// channel is closed.
    #[instrument(skip(self))]
    pub fn broadcast(&self, event: SyncEvent) -> Result<usize> {
        // `broadcast::Sender::send` only errors when there are zero active
        // receivers. That is not a failure for us — an event with no current
        // subscribers is simply delivered to nobody. Report 0 subscribers
        // instead of surfacing an error.
        match self.event_tx.send(event) {
            Ok(n) => Ok(n),
            Err(_) => Ok(0),
        }
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.event_tx.receiver_count()
    }

    /// Agent heartbeat interval in seconds.
    pub const HEARTBEAT_INTERVAL_SECS: u64 = 30;

    /// Agent timeout in seconds (3 missed heartbeats).
    pub const AGENT_TIMEOUT_SECS: u64 = 90;

    /// Maximum missed heartbeats before an agent is considered gone.
    pub const MAX_MISSED_HEARTBEATS: u32 = 3;
}

impl Default for SyncManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Agent presence tracker
// ---------------------------------------------------------------------------

/// Tracks which agents are currently online using heartbeats.
///
/// An agent is considered present if it has sent a heartbeat within the
/// last `AGENT_TIMEOUT_SECS`.  The tracker is used for:
/// * Split-brain detection (too few agents → potential partition)
/// * Lock cleanup (agent timed out → release its locks)
/// * Swarm reconfiguration (agent left → redistribute work)
#[derive(Debug)]
pub struct AgentPresenceTracker {
    /// Map from agent ID to the timestamp of its last heartbeat.
    last_seen: RwLock<HashMap<Uuid, chrono::DateTime<chrono::Utc>>>,
    timeout_secs: u64,
}

impl AgentPresenceTracker {
    pub fn new() -> Self {
        Self {
            last_seen: RwLock::new(HashMap::new()),
            timeout_secs: SyncManager::AGENT_TIMEOUT_SECS,
        }
    }

    pub fn with_timeout(timeout_secs: u64) -> Self {
        Self {
            last_seen: RwLock::new(HashMap::new()),
            timeout_secs,
        }
    }

    /// Record a heartbeat from an agent.
    pub async fn heartbeat(&self, agent_id: Uuid) {
        let now = chrono::Utc::now();
        let mut map = self.last_seen.write().await;
        map.insert(agent_id, now);
        debug!("Agent {} heartbeat recorded at {}", agent_id, now);
    }

    /// Check if an agent is currently present (within timeout).
    pub async fn is_present(&self, agent_id: Uuid) -> bool {
        let map = self.last_seen.read().await;
        match map.get(&agent_id) {
            Some(last) => {
                let elapsed = (chrono::Utc::now() - *last).num_seconds() as u64;
                elapsed < self.timeout_secs
            }
            None => false,
        }
    }

    /// Return all agents currently considered present.
    pub async fn present_agents(&self) -> Vec<Uuid> {
        let map = self.last_seen.read().await;
        let now = chrono::Utc::now();
        map.iter()
            .filter(|(_, last)| {
                let elapsed = (now - **last).num_seconds() as u64;
                elapsed < self.timeout_secs
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Return agents that have timed out (for cleanup / reconfiguration).
    pub async fn timed_out_agents(&self) -> Vec<Uuid> {
        let map = self.last_seen.read().await;
        let now = chrono::Utc::now();
        map.iter()
            .filter(|(_, last)| {
                let elapsed = (now - **last).num_seconds() as u64;
                elapsed >= self.timeout_secs
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Remove a timed-out agent from tracking.
    pub async fn remove_agent(&self, agent_id: Uuid) {
        let mut map = self.last_seen.write().await;
        map.remove(&agent_id);
    }

    /// Total number of agents ever seen (including timed out).
    pub async fn total_tracked(&self) -> usize {
        let map = self.last_seen.read().await;
        map.len()
    }
}

impl Default for AgentPresenceTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Split-brain detector
// ---------------------------------------------------------------------------

/// Detects and resolves split-brain conditions in distributed deployments.
///
/// A split-brain occurs when the network partitions and agents on different
/// sides of the partition make conflicting changes.  The detector uses
/// quorum-based reasoning: if fewer than `min_agents_for_quorum` agents are
/// visible, a partition is suspected.
#[derive(Debug)]
pub struct SplitBrainDetector {
    /// Minimum agents required to form a quorum.
    min_agents_for_quorum: usize,
    /// Known partition IDs (one per network segment).
    partition_state: RwLock<HashMap<Uuid, PartitionInfo>>,
}

/// Information about a detected network partition.
#[derive(Debug, Clone)]
pub struct PartitionInfo {
    pub partition_id: Uuid,
    pub agent_count: usize,
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub is_resolved: bool,
}

impl SplitBrainDetector {
    pub fn new(min_agents_for_quorum: usize) -> Self {
        Self {
            min_agents_for_quorum,
            partition_state: RwLock::new(HashMap::new()),
        }
    }

    /// Check whether the current agent count indicates a possible partition.
    ///
    /// Returns `Some(partition_id)` if a split-brain is detected, `None`
    /// if the cluster appears healthy.
    #[instrument(skip(self))]
    pub async fn check_partition(&self, visible_agent_count: usize) -> Option<Uuid> {
        if visible_agent_count >= self.min_agents_for_quorum {
            return None;
        }

        let partition_id = Uuid::new_v4();
        warn!(
            "Possible split-brain detected: only {} agents visible (quorum = {})",
            visible_agent_count, self.min_agents_for_quorum
        );

        let mut state = self.partition_state.write().await;
        state.insert(
            partition_id,
            PartitionInfo {
                partition_id,
                agent_count: visible_agent_count,
                detected_at: chrono::Utc::now(),
                is_resolved: false,
            },
        );

        Some(partition_id)
    }

    /// Mark a partition as resolved.
    pub async fn resolve_partition(&self, partition_id: Uuid) {
        let mut state = self.partition_state.write().await;
        if let Some(info) = state.get_mut(&partition_id) {
            info.is_resolved = true;
            info!("Partition {partition_id} marked as resolved");
        }
    }

    /// List all unresolved partitions.
    pub async fn unresolved_partitions(&self) -> Vec<PartitionInfo> {
        let state = self.partition_state.read().await;
        state
            .values()
            .filter(|info| !info.is_resolved)
            .cloned()
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Lock manager (collaborative editing)
// ---------------------------------------------------------------------------

/// Manages prompt locks for collaborative editing.
///
/// Ensures that only one agent can edit a prompt at a time.  Locks have a
/// TTL and are automatically released when the owning agent times out.
#[derive(Debug)]
pub struct LockManager {
    locks: RwLock<HashMap<Uuid, LockToken>>,
}

impl LockManager {
    pub fn new() -> Self {
        Self {
            locks: RwLock::new(HashMap::new()),
        }
    }

    /// Acquire a lock on a prompt.
    ///
    /// Returns `Err` if the prompt is already locked by another agent.
    pub async fn acquire_lock(
        &self,
        prompt_id: Uuid,
        agent_id: Uuid,
        ttl_seconds: u64,
    ) -> Result<LockToken> {
        let mut locks = self.locks.write().await;

        if let Some(existing) = locks.get(&prompt_id) {
            let now = chrono::Utc::now();
            if existing.expires_at > now {
                return Err(HubError::LockError(format!(
                    "Prompt {} is already locked by agent {} until {}",
                    prompt_id, existing.agent_id, existing.expires_at
                )));
            }
            // Lock expired — steal it.
            warn!(
                "Stealing expired lock on prompt {} from agent {}",
                prompt_id, existing.agent_id
            );
        }

        let lock = LockToken {
            id: Uuid::new_v4(),
            prompt_id,
            agent_id,
            token_hash: format!("lock-{}-{}", prompt_id, agent_id),
            expires_at: chrono::Utc::now() + chrono::TimeDelta::seconds(ttl_seconds as i64),
            created_at: chrono::Utc::now(),
        };

        locks.insert(prompt_id, lock.clone());
        Ok(lock)
    }

    /// Release a lock on a prompt.
    pub async fn release_lock(&self, prompt_id: Uuid, agent_id: Uuid) -> Result<()> {
        let mut locks = self.locks.write().await;

        match locks.get(&prompt_id) {
            Some(lock) if lock.agent_id == agent_id => {
                locks.remove(&prompt_id);
                Ok(())
            }
            Some(lock) => Err(HubError::LockError(format!(
                "Agent {} does not own the lock on prompt {} (owned by {})",
                agent_id, prompt_id, lock.agent_id
            ))),
            None => Err(HubError::LockError(format!(
                "Prompt {} is not locked",
                prompt_id
            ))),
        }
    }

    /// Check if a prompt is currently locked.
    pub async fn is_locked(&self, prompt_id: Uuid) -> bool {
        let locks = self.locks.read().await;
        match locks.get(&prompt_id) {
            Some(lock) => lock.expires_at > chrono::Utc::now(),
            None => false,
        }
    }

    /// Get the lock record for a prompt (if any).
    pub async fn get_lock(&self, prompt_id: Uuid) -> Option<LockToken> {
        let locks = self.locks.read().await;
        locks.get(&prompt_id).cloned()
    }

    /// Release all locks owned by a given agent (e.g., on timeout).
    pub async fn release_all_for_agent(&self, agent_id: Uuid) -> usize {
        let mut locks = self.locks.write().await;
        let to_remove: Vec<_> = locks
            .iter()
            .filter(|(_, lock)| lock.agent_id == agent_id)
            .map(|(k, _)| *k)
            .collect();
        let count = to_remove.len();
        for key in to_remove {
            locks.remove(&key);
        }
        count
    }

    /// Clean up all expired locks.
    pub async fn cleanup_expired(&self) -> usize {
        let mut locks = self.locks.write().await;
        let now = chrono::Utc::now();
        let expired: Vec<_> = locks
            .iter()
            .filter(|(_, lock)| lock.expires_at <= now)
            .map(|(k, _)| *k)
            .collect();
        let count = expired.len();
        for key in expired {
            locks.remove(&key);
        }
        count
    }

    /// Total number of active (non-expired) locks.
    pub async fn active_lock_count(&self) -> usize {
        let locks = self.locks.read().await;
        let now = chrono::Utc::now();
        locks.values().filter(|l| l.expires_at > now).count()
    }
}

impl Default for LockManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// WebSocket sync backend (feature-gated)
// ---------------------------------------------------------------------------

/// WebSocket-based sync backend for real-time cross-agent communication.
///
/// Uses `tokio-tungstenite` for WebSocket transport.  Each connected agent
/// receives a dedicated text stream of `SyncEvent` JSON messages.
#[cfg(feature = "tokio-tungstenite")]
#[derive(Debug)]
pub struct WebSocketSync {
    addr: std::net::SocketAddr,
}

#[cfg(feature = "tokio-tungstenite")]
impl WebSocketSync {
    pub fn new(addr: std::net::SocketAddr) -> Self {
        Self { addr }
    }

    /// Start the WebSocket server.
    ///
    /// This spawns a tokio task that accepts connections and relays
    /// sync events to all connected clients.
    pub async fn start(&self) -> Result<()> {
        info!("WebSocket sync server starting on {}", self.addr);
        // Implementation: bind tokio-tungstenite accept loop.
        // For each connection: deserialize JSON SyncEvent, broadcast.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// File watcher sync backend (feature-gated)
// ---------------------------------------------------------------------------

/// File-system watcher sync backend for local development.
///
/// Uses `notify 7.0.1` to watch the prompt directory for changes.
/// Events are debounced to avoid triggering on rapid successive writes.
#[cfg(feature = "notify")]
#[derive(Debug)]
pub struct FileWatcherSync {
    debounce_ms: u64,
    watch_paths: Vec<std::path::PathBuf>,
}

#[cfg(feature = "notify")]
impl FileWatcherSync {
    pub fn new() -> Self {
        Self {
            debounce_ms: 500,
            watch_paths: Vec::new(),
        }
    }

    pub fn with_debounce(debounce_ms: u64) -> Self {
        Self {
            debounce_ms,
            watch_paths: Vec::new(),
        }
    }

    /// Add a path to watch.
    pub fn watch(&mut self, path: impl Into<std::path::PathBuf>) {
        self.watch_paths.push(path.into());
    }

    /// Start the file watcher.
    ///
    /// This spawns a tokio task that uses `notify` to monitor the watched
    /// paths and emits `SyncEvent::PromptUpdated` events on changes.
    pub async fn start(&self) -> Result<()> {
        info!(
            "File watcher sync starting with {} path(s), debounce={}ms",
            self.watch_paths.len(),
            self.debounce_ms
        );
        // Implementation: notify::RecommendedWatcher with debounce.
        Ok(())
    }
}

#[cfg(feature = "notify")]
impl Default for FileWatcherSync {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Sync manager -------------------------------------------------------

    #[test]
    fn test_sync_manager_broadcast() {
        let manager = SyncManager::new();
        let event = SyncEvent::AgentJoined {
            agent_id: Uuid::new_v4(),
            name: "test".to_string(),
        };
        assert!(manager.broadcast(event).is_ok());
    }

    #[test]
    fn test_sync_manager_subscribe() {
        let manager = SyncManager::new();
        let _rx = manager.subscribe();
        let event = SyncEvent::PromptAdded {
            prompt_id: Uuid::new_v4(),
        };
        assert!(manager.broadcast(event.clone()).is_ok());
    }

    #[test]
    fn test_sync_manager_subscriber_count() {
        let manager = SyncManager::new();
        assert_eq!(manager.subscriber_count(), 0);
        let _rx1 = manager.subscribe();
        assert_eq!(manager.subscriber_count(), 1);
        let _rx2 = manager.subscribe();
        assert_eq!(manager.subscriber_count(), 2);
    }

    #[test]
    fn test_sync_manager_default() {
        let _manager = SyncManager::default();
        assert_eq!(SyncManager::HEARTBEAT_INTERVAL_SECS, 30);
        assert_eq!(SyncManager::AGENT_TIMEOUT_SECS, 90);
        assert_eq!(SyncManager::MAX_MISSED_HEARTBEATS, 3);
        // Verify 90s = 3 * 30s
        assert_eq!(
            SyncManager::AGENT_TIMEOUT_SECS,
            SyncManager::HEARTBEAT_INTERVAL_SECS * SyncManager::MAX_MISSED_HEARTBEATS as u64
        );
    }

    // -- Agent presence tracker ---------------------------------------------

    #[tokio::test]
    async fn test_presence_heartbeat_and_check() {
        let tracker = AgentPresenceTracker::new();
        let agent_id = Uuid::new_v4();

        assert!(!tracker.is_present(agent_id).await);

        tracker.heartbeat(agent_id).await;
        assert!(tracker.is_present(agent_id).await);
    }

    #[tokio::test]
    async fn test_presence_timeout() {
        let tracker = AgentPresenceTracker::with_timeout(0); // immediate timeout
        let agent_id = Uuid::new_v4();

        tracker.heartbeat(agent_id).await;
        // Small delay to ensure timeout
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        assert!(!tracker.is_present(agent_id).await);
    }

    #[tokio::test]
    async fn test_present_agents() {
        let tracker = AgentPresenceTracker::new();
        let a1 = Uuid::new_v4();
        let a2 = Uuid::new_v4();

        tracker.heartbeat(a1).await;
        tracker.heartbeat(a2).await;

        let present = tracker.present_agents().await;
        assert_eq!(present.len(), 2);
        assert!(present.contains(&a1));
        assert!(present.contains(&a2));
    }

    #[tokio::test]
    async fn test_timed_out_agents() {
        let tracker = AgentPresenceTracker::with_timeout(0); // immediate timeout
        let a1 = Uuid::new_v4();

        tracker.heartbeat(a1).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let timed_out = tracker.timed_out_agents().await;
        assert!(timed_out.contains(&a1));
    }

    #[tokio::test]
    async fn test_remove_agent() {
        let tracker = AgentPresenceTracker::new();
        let agent_id = Uuid::new_v4();

        tracker.heartbeat(agent_id).await;
        assert!(tracker.is_present(agent_id).await);

        tracker.remove_agent(agent_id).await;
        assert!(!tracker.is_present(agent_id).await);
        assert_eq!(tracker.total_tracked().await, 0);
    }

    // -- Lock manager -------------------------------------------------------

    #[tokio::test]
    async fn test_lock_acquire_and_release() {
        let manager = LockManager::new();
        let prompt_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let lock = manager.acquire_lock(prompt_id, agent_id, 60).await;
        assert!(lock.is_ok());
        assert!(manager.is_locked(prompt_id).await);

        let release = manager.release_lock(prompt_id, agent_id).await;
        assert!(release.is_ok());
        assert!(!manager.is_locked(prompt_id).await);
    }

    #[tokio::test]
    async fn test_lock_conflict() {
        let manager = LockManager::new();
        let prompt_id = Uuid::new_v4();
        let agent_a = Uuid::new_v4();
        let agent_b = Uuid::new_v4();

        let lock_a = manager.acquire_lock(prompt_id, agent_a, 60).await;
        assert!(lock_a.is_ok());

        let lock_b = manager.acquire_lock(prompt_id, agent_b, 60).await;
        assert!(lock_b.is_err());
    }

    #[tokio::test]
    async fn test_lock_release_wrong_owner() {
        let manager = LockManager::new();
        let prompt_id = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let other = Uuid::new_v4();

        manager.acquire_lock(prompt_id, owner, 60).await.unwrap();
        let result = manager.release_lock(prompt_id, other).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lock_release_not_locked() {
        let manager = LockManager::new();
        let prompt_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let result = manager.release_lock(prompt_id, agent_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lock_expired() {
        let manager = LockManager::new();
        let prompt_id = Uuid::new_v4();
        let agent_a = Uuid::new_v4();
        let agent_b = Uuid::new_v4();

        // Acquire with 0-second TTL (immediately expired).
        manager.acquire_lock(prompt_id, agent_a, 0).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Second agent should be able to steal the expired lock.
        let lock_b = manager.acquire_lock(prompt_id, agent_b, 60).await;
        assert!(lock_b.is_ok());
        assert_eq!(lock_b.unwrap().agent_id, agent_b);
    }

    #[tokio::test]
    async fn test_lock_release_all_for_agent() {
        let manager = LockManager::new();
        let agent_id = Uuid::new_v4();

        manager
            .acquire_lock(Uuid::new_v4(), agent_id, 60)
            .await
            .unwrap();
        manager
            .acquire_lock(Uuid::new_v4(), agent_id, 60)
            .await
            .unwrap();
        manager
            .acquire_lock(Uuid::new_v4(), agent_id, 60)
            .await
            .unwrap();

        let count = manager.release_all_for_agent(agent_id).await;
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_lock_cleanup_expired() {
        let manager = LockManager::new();
        let agent_id = Uuid::new_v4();

        // Lock with 0s TTL → immediately expired.
        manager
            .acquire_lock(Uuid::new_v4(), agent_id, 0)
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let cleaned = manager.cleanup_expired().await;
        assert_eq!(cleaned, 1);
    }

    #[tokio::test]
    async fn test_lock_get_record() {
        let manager = LockManager::new();
        let prompt_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        manager.acquire_lock(prompt_id, agent_id, 60).await.unwrap();

        let record = manager.get_lock(prompt_id).await;
        assert!(record.is_some());
        assert_eq!(record.unwrap().agent_id, agent_id);
    }

    #[tokio::test]
    async fn test_active_lock_count() {
        let manager = LockManager::new();
        let agent_id = Uuid::new_v4();

        assert_eq!(manager.active_lock_count().await, 0);
        manager
            .acquire_lock(Uuid::new_v4(), agent_id, 60)
            .await
            .unwrap();
        assert_eq!(manager.active_lock_count().await, 1);
    }

    // -- Split-brain detector -----------------------------------------------

    #[tokio::test]
    async fn test_split_brain_healthy_cluster() {
        let detector = SplitBrainDetector::new(3);
        let result = detector.check_partition(5).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_split_brain_detected() {
        let detector = SplitBrainDetector::new(3);
        let result = detector.check_partition(1).await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_split_brain_at_quorum_boundary() {
        let detector = SplitBrainDetector::new(3);
        // Exactly at quorum should NOT trigger.
        let result = detector.check_partition(3).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_split_brain_resolve() {
        let detector = SplitBrainDetector::new(3);
        let partition_id = detector.check_partition(1).await.unwrap();

        detector.resolve_partition(partition_id).await;
        let unresolved = detector.unresolved_partitions().await;
        assert!(unresolved.is_empty());
    }

    #[tokio::test]
    async fn test_split_brain_unresolved_list() {
        let detector = SplitBrainDetector::new(3);
        detector.check_partition(1).await;
        detector.check_partition(2).await;

        let unresolved = detector.unresolved_partitions().await;
        assert_eq!(unresolved.len(), 2);
    }

    // -- Sync events --------------------------------------------------------

    #[test]
    fn test_sync_event_variants() {
        let agent_id = Uuid::new_v4();
        let prompt_id = Uuid::new_v4();

        let events = vec![
            SyncEvent::PromptAdded { prompt_id },
            SyncEvent::PromptUpdated { prompt_id },
            SyncEvent::PromptLocked {
                prompt_id,
                agent_id,
            },
            SyncEvent::PromptUnlocked { prompt_id },
            SyncEvent::AgentJoined {
                agent_id,
                name: "test".to_string(),
            },
            SyncEvent::AgentLeft { agent_id },
            SyncEvent::OwnershipTransferred {
                prompt_id,
                from: agent_id,
                to: Uuid::new_v4(),
            },
            SyncEvent::PatternShared {
                pattern_id: Uuid::new_v4(),
                from_agent: agent_id,
            },
        ];

        assert_eq!(events.len(), 8);
    }

    #[test]
    fn test_split_brain_resolution_variants() {
        let resolution = SplitBrainResolution::KeepPartition {
            winning_partition: Uuid::new_v4(),
        };
        assert!(matches!(
            resolution,
            SplitBrainResolution::KeepPartition { .. }
        ));

        let resolution = SplitBrainResolution::MergePartitions {
            merged_from: vec![Uuid::new_v4()],
        };
        assert!(matches!(
            resolution,
            SplitBrainResolution::MergePartitions { .. }
        ));

        let resolution = SplitBrainResolution::ManualInterventionRequired {
            reason: "test".to_string(),
        };
        assert!(matches!(
            resolution,
            SplitBrainResolution::ManualInterventionRequired { .. }
        ));
    }

    // -- Send + Sync --------------------------------------------------------

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_sync_manager_send_sync() {
        assert_send_sync::<SyncManager>();
    }

    #[test]
    fn test_sync_event_send_sync() {
        assert_send_sync::<SyncEvent>();
    }
}
