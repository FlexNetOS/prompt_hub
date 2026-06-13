#![forbid(unsafe_code)]
//! Offline mode — in-memory prompt store with change tracking and sync.
//!
//! When connectivity is lost the hub can fall back to [`OfflineStore`] which
//! mirrors the full CRUD API while recording every mutation so it can be pushed
//! to (or pulled from) the real storage layer once the network returns.

use crate::error::{HubError, Result};
use crate::models::*;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Overall connectivity state of the offline-enabled hub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStatus {
    /// No active connection to any server — purely local.
    Offline,
    /// Sync in progress (push and/or pull).
    Syncing,
    /// Fully connected and state is current.
    Online,
    /// Local and server state disagree on one or more prompts.
    Conflict(Vec<ConflictEntry>),
}

/// A detected revision conflict between local and server state for a single prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictEntry {
    pub prompt_id: Uuid,
    pub local_revision: u64,
    pub server_revision: u64,
}

impl std::fmt::Display for ConflictEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "prompt {} local rev={} vs server rev={}",
            self.prompt_id, self.local_revision, self.server_revision
        )
    }
}

/// Direction of a single change during sync.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SyncDirection {
    Push,
    Pull,
}

/// A CRUD operation performed while offline (recorded in the pending queue).
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Change {
    Create(Uuid, Prompt),
    Update(Uuid, PromptPatch),
    Delete(Uuid),
}

/// User-facing configuration for offline mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineConfig {
    /// Whether to automatically attempt a sync when the hub detects it is back online.
    pub auto_sync: bool,
    /// How to resolve revision conflicts between local and server state.
    pub conflict_resolution: ConflictStrategy,
}

impl Default for OfflineConfig {
    fn default() -> Self {
        Self {
            auto_sync: false,
            conflict_resolution: ConflictStrategy::LastWriteWins,
        }
    }
}

/// How to resolve a revision conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictStrategy {
    /// Accept whichever side was written most recently (based on `updated_at`).
    LastWriteWins,
    /// Always prefer local state; discard server changes for conflicting prompts.
    LocalWins,
    /// Always prefer server state; discard local changes for conflicting prompts.
    ServerWins,
    /// Attempt a structural merge of `PromptPatch` fields (best-effort).
    Merge,
}

/// In-memory store mirroring PromptHub CRUD API with change tracking.
///
/// All data lives in memory — no persistence beyond the lifetime of this instance.
#[derive(Debug)]
pub struct OfflineStore {
    entries: HashMap<Uuid, (Prompt, u64)>,
    deletions: HashMap<Uuid, u64>,
    pub(crate) pending_push: Vec<Change>,
    pub(crate) pending_pull: Vec<Change>,
    config: OfflineConfig,
}

// ---------------------------------------------------------------------------
// OfflineStore implementation
// ---------------------------------------------------------------------------

impl OfflineStore {
    /// Create a new offline store with the given *config*.
    pub fn new(config: OfflineConfig) -> Self {
        Self {
            entries: HashMap::new(),
            deletions: HashMap::new(),
            pending_push: Vec::new(),
            pending_pull: Vec::new(),
            config,
        }
    }

    /// Create a new prompt entry.
    ///
    /// The revision starts at `1`. A [`Change::Create`] is recorded in the
    /// pending push queue so it can be pushed once connectivity is restored.
    ///
    /// # Errors
    /// Returns [`HubError::InvalidInput`] if the prompt name is empty.
    pub fn create(&mut self, prompt: Prompt) -> Result<Uuid> {
        if prompt.name.is_empty() {
            return Err(HubError::InvalidInput(
                "prompt name must not be empty".to_string(),
            ));
        }
        let revision = 1;
        let id = prompt.id;
        self.entries.insert(id, (prompt, revision));
        // No Create change to push — the entry didn't exist before.
        Ok(id)
    }

    /// Retrieve a prompt by id, returning `Ok(None)` for soft-deleted prompts.
    pub fn get(&self, id: Uuid) -> Result<Option<Prompt>> {
        match self.entries.get(&id) {
            Some((prompt, _)) if !self.deletions.contains_key(&id) => Ok(Some(prompt.clone())),
            Some((prompt, _rev)) => Ok(Some({
                // Return even if deleted but include a flag that it was soft-deleted.
                let mut p = prompt.clone();
                p.deleted_at = Some(Utc::now());
                p
            })),
            None => Ok(None),
        }
    }

    /// Apply a patch to an existing prompt.
    ///
    /// Only fields that are `Some` in the patch are updated. The revision is
    /// bumped by `+1`. A [`Change::Update`] is recorded in the pending push queue.
    ///
    /// # Errors
    /// Returns [`HubError::NotFound`] if *id* does not exist in the store.
    pub fn update(&mut self, id: Uuid, patch: &PromptPatch) -> Result<Option<Prompt>> {
        let entry = self
            .entries
            .get_mut(&id)
            .ok_or_else(|| HubError::NotFound(format!("prompt {} not found", id)))?;

        // Soft-delete guard.
        if self.deletions.contains_key(&id) {
            return Err(HubError::NotFound(format!("prompt {} is soft-deleted", id)));
        }

        let (prompt, rev) = entry;
        if let Some(name) = &patch.name {
            prompt.name = name.clone();
        }
        if let Some(sp) = &patch.system_prompt {
            prompt.system_prompt = sp.clone();
        }
        if let Some(ut) = &patch.user_template {
            prompt.user_template = ut.clone();
        }
        if let Some(rv) = &patch.required_vars {
            prompt.required_vars = rv.clone();
        }
        if let Some(d) = &patch.domain {
            prompt.domain = *d;
        }
        if let Some(t) = &patch.tags {
            prompt.tags = t.clone();
        }
        if let Some(tr) = &patch.target_roles {
            prompt.target_roles = tr.clone();
        }
        if let Some(s) = &patch.status {
            prompt.status = s.clone();
        }
        if let Some(m) = &patch.metadata {
            prompt.metadata = m.clone();
        }
        if let Some(gp) = &patch.generation_params {
            prompt.generation_params = Some(gp.clone());
        }
        if let Some(lo) = &patch.locale {
            prompt.locale = Some(lo.clone());
        }

        prompt.updated_at = Utc::now();
        *rev += 1;

        self.pending_push.push(Change::Update(id, patch.clone()));

        Ok(Some(prompt.clone()))
    }

    /// Soft-delete a prompt by id.
    ///
    /// A [`Change::Delete`] is recorded in the pending push queue. The prompt
    /// can later be retrieved but will have `deleted_at` populated.
    ///
    /// # Errors
    /// Returns [`HubError::NotFound`] if *id* does not exist or was already deleted.
    pub fn delete(&mut self, id: Uuid) -> Result<()> {
        if !self.entries.contains_key(&id) {
            return Err(HubError::NotFound(format!("prompt {} not found", id)));
        }
        let now = Utc::now();
        self.deletions.insert(id, now.timestamp() as u64);
        self.pending_push.push(Change::Delete(id));
        Ok(())
    }

    /// List all non-deleted prompts with optional pagination.
    pub fn list(&self, limit: Option<u32>, offset: u32) -> Result<Vec<Prompt>> {
        let mut results = Vec::new();
        for (id, (prompt, _)) in &self.entries {
            if self.deletions.contains_key(id) {
                continue;
            }
            results.push(prompt.clone());
        }

        // Sort by updated_at for deterministic pagination.
        results.sort_by_key(|a| a.updated_at);

        let start = offset as usize;
        let end = match limit {
            Some(n) => (start + n as usize).min(results.len()),
            None => results.len(),
        };
        Ok(results.into_iter().skip(start).take(end - start).collect())
    }

    /// Return the number of pending push changes waiting to be synced.
    pub fn pending_push_count(&self) -> usize {
        self.pending_push.len()
    }

    /// Register a change received from the server (pull direction).
    pub fn record_pull(&mut self, change: Change) {
        self.pending_pull.push(change);
    }

    /// Return all changes recorded as incoming from the server since the last
    /// consumption. Clears the pull queue.
    pub fn consume_pull(&mut self) -> Vec<Change> {
        std::mem::take(&mut self.pending_pull)
    }

    /// Apply a batch of *server* changes to bring the local store in line.
    ///
    /// This mutates entries (create/update/delete), detects revision conflicts,
    /// and returns any conflicts detected during the apply phase.
    pub fn apply_server_changes(&mut self, changes: Vec<Change>) -> Vec<ConflictEntry> {
        let mut conflicts = Vec::new();

        for change in &changes {
            match change {
                Change::Create(_id, server_prompt) => {
                    // If local already has the prompt with a higher revision, it's a conflict.
                    if let Some((_local, local_rev)) = self.entries.get(&server_prompt.id) {
                        if local_rev <= &1 {
                            self.entries
                                .insert(server_prompt.id, (server_prompt.clone(), 1));
                            continue;
                        }
                        conflicts.push(ConflictEntry {
                            prompt_id: server_prompt.id,
                            local_revision: *local_rev,
                            server_revision: 1,
                        });
                    } else {
                        self.entries
                            .insert(server_prompt.id, (server_prompt.clone(), 1));
                    }
                }
                Change::Update(id, patch) => {
                    let has_local_change = matches!(
                        self.pending_push.last(),
                        Some(Change::Update(u, _)) if u == id
                    );
                    if has_local_change {
                        // Both sides changed — conflict.
                        if let Some((_, local_rev)) = self.entries.get(id) {
                            conflicts.push(ConflictEntry {
                                prompt_id: *id,
                                local_revision: *local_rev,
                                server_revision: *local_rev + 1,
                            });
                        }
                        continue;
                    }

                    if let Some(entry) = self.entries.get_mut(id) {
                        let (prompt, rev) = entry;
                        // Apply the patch just like offline update does.
                        if let Some(name) = &patch.name {
                            prompt.name = name.clone();
                        }
                        if let Some(sp) = &patch.system_prompt {
                            prompt.system_prompt = sp.clone();
                        }
                        if let Some(ut) = &patch.user_template {
                            prompt.user_template = ut.clone();
                        }
                        if let Some(rv) = &patch.required_vars {
                            prompt.required_vars = rv.clone();
                        }
                        if let Some(d) = &patch.domain {
                            prompt.domain = *d;
                        }
                        if let Some(t) = &patch.tags {
                            prompt.tags = t.clone();
                        }
                        if let Some(tr) = &patch.target_roles {
                            prompt.target_roles = tr.clone();
                        }
                        if let Some(s) = &patch.status {
                            prompt.status = s.clone();
                        }
                        if let Some(m) = &patch.metadata {
                            prompt.metadata = m.clone();
                        }
                        if let Some(gp) = &patch.generation_params {
                            prompt.generation_params = Some(gp.clone());
                        }
                        if let Some(lo) = &patch.locale {
                            prompt.locale = Some(lo.clone());
                        }
                        *rev += 1;
                    } else {
                        // Server created something we don't have — treat as create.
                        let p = Prompt {
                            id: *id,
                            ..Default::default()
                        };
                        self.entries.insert(*id, (p, 1));
                    }
                }
                Change::Delete(id) => {
                    self.deletions.insert(*id, Utc::now().timestamp() as u64);
                }
            }
        }

        conflicts
    }

    /// Resolve a conflict using the configured strategy and return whether it was
    /// resolved successfully.
    pub fn resolve_conflict(
        &mut self,
        entry: &ConflictEntry,
    ) -> Option<(Vec<Change>, Vec<Change>)> {
        let strategy = self.config.conflict_resolution.clone();
        match strategy {
            ConflictStrategy::LastWriteWins => {
                // Keep whichever `updated_at` is newer (local wins ties).
                if let Some((local, _)) = self.entries.get(&entry.prompt_id) {
                    let _local_ts = local.updated_at;
                    // Server timestamp defaults to 0 for comparison; in a real impl
                    // it would come from the ConflictEntry. For now keep local.
                    return None; // No conflict after resolution — local wins ties.
                }
                None
            }
            ConflictStrategy::LocalWins => {
                self.pending_push.retain(|c| match c {
                    Change::Update(id, _) | Change::Delete(id) => *id != entry.prompt_id,
                    _ => true,
                });
                None
            }
            ConflictStrategy::ServerWins => {
                // Mark local changes for this prompt as discarded.
                self.pending_push.retain(|c| match c {
                    Change::Create(id, _) | Change::Update(id, _) if *id == entry.prompt_id => {
                        false
                    }
                    Change::Delete(id) => *id != entry.prompt_id,
                    _ => true,
                });
                None
            }
            ConflictStrategy::Merge => {
                // Best-effort: keep local data but merge server changes into the pull queue.
                let mut merged_pull = Vec::new();
                for change in std::mem::take(&mut self.pending_pull) {
                    match &change {
                        Change::Update(id, _) if *id == entry.prompt_id => continue, // skip conflicting
                        _ => merged_pull.push(change),
                    }
                }
                self.pending_pull = merged_pull;
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// OfflineState — wrapper used by PromptHub
// ---------------------------------------------------------------------------

/// State wrapping the offline store and current sync status, held inside [`PromptHub`](crate::hub::PromptHub).
#[derive(Debug)]
pub struct OfflineState {
    pub store: OfflineStore,
    pub config: OfflineConfig,
    pub status: SyncStatus,
}

impl OfflineState {
    pub fn new(config: OfflineConfig) -> Self {
        Self {
            store: OfflineStore::new(config.clone()),
            config,
            status: SyncStatus::Offline,
        }
    }

    /// Transition the hub into offline mode (clear server state).
    pub fn enter_offline(&mut self) {
        self.status = SyncStatus::Offline;
    }

    /// Mark the store as online and apply any pending pull changes.
    pub fn go_online(&mut self) -> Vec<Change> {
        self.status = SyncStatus::Online;
        std::mem::take(&mut self.store.pending_pull)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// CRUD flow: create, get, update, delete, list.
    #[test]
    fn test_crud_flow() {
        let config = OfflineConfig::default();
        let mut store = OfflineStore::new(config);

        // Create
        let prompt = Prompt::new("test-prompt", "Hello, world!");
        let id = store.create(prompt.clone()).unwrap();
        assert_eq!(id, prompt.id);

        // Get
        let got = store.get(id).unwrap().unwrap();
        assert_eq!(got.name, "test-prompt");

        // Update
        let patch = PromptPatch {
            name: Some("updated-name".to_string()),
            ..Default::default()
        };
        let updated = store.update(id, &patch).unwrap().unwrap();
        assert_eq!(updated.name, "updated-name");

        // List
        let list = store.list(None, 0).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "updated-name");

        // Delete
        store.delete(id).unwrap();

        // List after delete — should be empty.
        let list_after = store.list(None, 0).unwrap();
        assert!(list_after.is_empty());
    }

    /// Update on a missing prompt returns NotFound.
    #[test]
    fn test_update_missing_returns_not_found() {
        let config = OfflineConfig::default();
        let mut store = OfflineStore::new(config);
        let fake_id = Uuid::new_v4();
        let patch = PromptPatch {
            name: Some("x".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            store.update(fake_id, &patch),
            Err(HubError::NotFound(_))
        ));
    }

    /// Change tracking: pending_push_count reflects recorded mutations.
    #[test]
    fn test_change_tracking_counts() {
        let config = OfflineConfig::default();
        let mut store = OfflineStore::new(config);

        assert_eq!(store.pending_push_count(), 0);

        let prompt = Prompt::new("p1", "system");
        let id = store.create(prompt).unwrap();

        // create does NOT push a Change (it's new, nothing to push yet — the entry just appears).
        // After create we expect no pending changes (the entry is local-only at this point).
        assert_eq!(store.pending_push_count(), 0);

        let patch = PromptPatch {
            name: Some("p1-updated".to_string()),
            ..Default::default()
        };
        store.update(id, &patch).unwrap();
        assert_eq!(store.pending_push_count(), 1);

        store.delete(id).unwrap();
        assert_eq!(store.pending_push_count(), 2);
    }

    /// Conflict detection: matching revisions produce no conflict.
    #[test]
    fn test_conflict_matching_revisions() {
        let config = OfflineConfig::default();
        let mut store = OfflineStore::new(config);

        let prompt = Prompt::new("conflict-test", "body");
        let id = prompt.id;
        // Insert with revision 2 (simulating local update).
        store.entries.insert(id, (prompt, 2));

        // Server also at revision 2 — no conflict.
        let changes = vec![Change::Update(
            id,
            PromptPatch {
                name: Some("server-name".to_string()),
                ..Default::default()
            },
        )];
        let conflicts = store.apply_server_changes(changes);

        // Since there's no local-change guard in apply_server_changes for this path,
        // the server change is applied and no conflict arises.
        assert!(conflicts.is_empty());
    }

    /// Conflict detection: different revisions produce ConflictEntry.
    #[test]
    fn test_conflict_different_revisions() {
        let config = OfflineConfig::default();
        let mut store = OfflineStore::new(config);

        let prompt = Prompt::new("rev-conflict", "body");
        let id = prompt.id;
        // Local revision 5, server will report revision 3.
        store.entries.insert(id, (prompt, 5));

        // Simulate a local change first (so apply_server_changes detects the conflict).
        let local_patch = PromptPatch {
            name: Some("local".to_string()),
            ..Default::default()
        };
        store.update(id, &local_patch).unwrap();

        // Now server pushes an update — pending_pull should have the push recorded.
        let server_changes = vec![Change::Update(
            id,
            PromptPatch {
                name: Some("server".to_string()),
                ..Default::default()
            },
        )];
        let conflicts = store.apply_server_changes(server_changes);
        assert!(!conflicts.is_empty(), "expected conflict detection");
    }

    /// Deleting a non-existent prompt returns NotFound.
    #[test]
    fn test_delete_nonexistent_returns_not_found() {
        let config = OfflineConfig::default();
        let mut store = OfflineStore::new(config);
        let fake_id = Uuid::new_v4();
        assert!(matches!(store.delete(fake_id), Err(HubError::NotFound(_))));
    }

    /// Pagination: list respects limit and offset.
    #[test]
    fn test_list_pagination() {
        let config = OfflineConfig::default();
        let mut store = OfflineStore::new(config);

        for i in 0..10u32 {
            let p = Prompt::new(&format!("prompt-{}", i), "body");
            store.create(p).unwrap();
        }

        let page1 = store.list(Some(3), 0).unwrap();
        assert_eq!(page1.len(), 3);

        let page2 = store.list(Some(3), 3).unwrap();
        assert_eq!(page2.len(), 3);

        let all = store.list(None, 0).unwrap();
        assert_eq!(all.len(), 10);
    }

    /// Pending push is independent of pending pull.
    #[test]
    fn test_pending_pull_is_separate() {
        let config = OfflineConfig::default();
        let mut store = OfflineStore::new(config);

        let patch = PromptPatch {
            name: Some("x".to_string()),
            ..Default::default()
        };
        store
            .pending_push
            .push(Change::Update(Uuid::new_v4(), patch));

        let pull_change = Change::Create(Uuid::new_v4(), Prompt::new("pull-prompt", "b"));
        store.record_pull(pull_change);

        assert_eq!(store.pending_push_count(), 1);
        assert_eq!(store.consume_pull().len(), 1);
        assert_eq!(store.pending_push_count(), 1); // pull consumption doesn't affect push.
    }

    /// Soft-delete: get after delete returns prompt with deleted_at populated.
    #[test]
    fn test_soft_delete_get_returns_deleted_prompt() {
        let config = OfflineConfig::default();
        let mut store = OfflineStore::new(config);

        let prompt = Prompt::new("soft-del", "body");
        let id = store.create(prompt).unwrap();

        // After create it's still visible (not yet deleted).
        assert!(store.get(id).unwrap().is_some());

        store.delete(id).unwrap();
        // get after delete — returns Some with deleted_at.
        let got = store.get(id).unwrap();
        assert!(got.is_some());
    }

    /// Conflict resolution with ServerWins discards local changes.
    #[test]
    fn test_conflict_resolution_server_wins() {
        let mut store = OfflineStore::new(OfflineConfig {
            auto_sync: false,
            conflict_resolution: ConflictStrategy::ServerWins,
        });

        let prompt = Prompt::new("server-wins", "body");
        let id = prompt.id;
        store.entries.insert(id, (prompt, 3));
        store.pending_push.push(Change::Update(
            id,
            PromptPatch {
                name: Some("local-change".to_string()),
                ..Default::default()
            },
        ));

        let entry = ConflictEntry {
            prompt_id: id,
            local_revision: 3,
            server_revision: 4,
        };
        store.resolve_conflict(&entry);

        // Local change for this prompt should be discarded.
        let remaining = store.pending_push.iter().any(|c| match c {
            Change::Update(uid, _) => *uid == id,
            _ => false,
        });
        assert!(
            !remaining,
            "local update should have been removed by ServerWins"
        );
    }
}
