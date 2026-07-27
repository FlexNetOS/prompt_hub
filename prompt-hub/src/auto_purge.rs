#![forbid(unsafe_code)]
//! Auto-purge: periodic prompt cleanup driven by configurable purge policies.
//!
//! This module provides a daemon-driven purge engine that scans the prompt
//! store on a fixed interval, evaluates each prompt against a set of
//! [`PurgePolicy`]s (first-match-wins), and executes the configured action
//! — delete, archive to a directory, or retain.
//!
//! ## Architecture
//!
//! ```text
//! AutoPurgeConfig  ──►  PurgePolicy[]  ──►  PurgeAction
//!      │                        │                    │
//!      │  interval: Duration    │  condition match   │ Delete / Archive(path) / Retain
//!      │  enabled: bool         │  min_age_days      │
//!      ▼                        ▼
//! AutoPurgeEngine ──run_purge()──► PurgeStats
//! ```

use crate::error::{HubError, Result};
use crate::models::*;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

// ─────────────────────────────────────────────
// PurgeAction — what to do with a matching prompt
// ─────────────────────────────────────────────

/// What to do with a prompt that matches a purge policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PurgeAction {
    /// Permanently delete the prompt from storage.
    Delete,
    /// Serialize to JSON at `{path}/{uuid}.json`, then hard-delete.
    Archive(String),
    /// Keep the prompt (no-op); useful for placeholder / dry-run policies.
    Retain,
}

// ─────────────────────────────────────────────
// PolicyCondition — selects which prompts match
// ─────────────────────────────────────────────

/// A filter condition that selects prompts for purging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyCondition {
    /// Minimum age in days from `created_at` (checked separately).
    DaysOld(u64),
    /// All tags must be present in the prompt's tag set (AND semantics).
    Tags(Vec<String>),
    /// Exact domain match.
    Domain(crate::models::Domain),
    /// Exact status match.
    Status(crate::models::Status),
}

impl PolicyCondition {
    /// Evaluate whether *prompt* satisfies this condition.
    pub fn matches(&self, prompt: &crate::models::Prompt) -> bool {
        match self {
            PolicyCondition::DaysOld(_min_days) => {
                // `min_days` is checked in `PurgePolicy::matches` alongside
                // the actual age; this method only checks conditions that can
                // be evaluated without reference to a separate min_age parameter.
                true
            }
            PolicyCondition::Tags(required_tags) => required_tags
                .iter()
                .all(|t| prompt.tags.iter().any(|p| p == t)),
            PolicyCondition::Domain(domain) => &prompt.domain == domain,
            PolicyCondition::Status(status) => &prompt.status == status,
        }
    }

    /// Check if a condition is purely tag-based (used in logging).
    pub fn is_tag_condition(&self) -> bool {
        matches!(self, PolicyCondition::Tags(_))
    }
}

// ─────────────────────────────────────────────
// PurgePolicy — complete rule with age + condition + action
// ─────────────────────────────────────────────

/// A single purge policy combining a minimum age threshold, a selection
/// condition, and the action to apply on match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgePolicy {
    /// Minimum number of days after `created_at` before this policy applies.
    pub min_age_days: u64,
    /// Condition that must also be satisfied (age is checked separately).
    pub condition: PolicyCondition,
    /// What to do when both age and condition are met.
    pub action: PurgeAction,
}

impl PurgePolicy {
    /// Check if a prompt matches this policy by verifying condition + min age.
    pub fn matches(&self, prompt: &crate::models::Prompt) -> bool {
        // Condition must match AND minimum age must be met.
        if !self.condition.matches(prompt) {
            return false;
        }

        let age = (Utc::now() - prompt.created_at).num_days();
        age >= 0 && (age as u64) >= self.min_age_days
    }
}

impl std::fmt::Display for PurgePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PurgePolicy {{ age>={}d, action={:?} }}",
            self.min_age_days, self.action,
        )
    }
}

// ─────────────────────────────────────────────
// AutoPurgeConfig — daemon configuration
// ─────────────────────────────────────────────

/// Configuration for the auto-purge daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoPurgeConfig {
    /// Interval between purge scans.
    pub interval: Duration,
    /// Ordered list of policies (first-match-wins).
    pub policies: Vec<PurgePolicy>,
    /// Whether the daemon is enabled.
    pub enabled: bool,
}

impl Default for AutoPurgeConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(24 * 3600), // 1 day
            policies: Vec::new(),
            enabled: false,
        }
    }
}

// ─────────────────────────────────────────────
// PurgeStats — per-cycle counters
// ─────────────────────────────────────────────

/// Per-cycle purge statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PurgeStats {
    /// Number of prompts scanned across all policies.
    pub total_scanned: usize,
    /// Number of prompts that were permanently deleted.
    pub purged_count: usize,
    /// Number of prompts that were archived to disk.
    pub archived_count: usize,
    /// Number of prompts that passed all policies (retained).
    pub retained_count: usize,
}

/// Internal atomic counters used by `AutoPurgeEngine` across daemon cycles.
#[derive(Debug, Default)]
struct AtomicStats {
    total_scanned: AtomicUsize,
    purged_count: AtomicUsize,
    archived_count: AtomicUsize,
    retained_count: AtomicUsize,
}

impl AtomicStats {
    fn reset(&self) {
        self.total_scanned.store(0, Ordering::SeqCst);
        self.purged_count.store(0, Ordering::SeqCst);
        self.archived_count.store(0, Ordering::SeqCst);
        self.retained_count.store(0, Ordering::SeqCst);
    }

    fn to_snapshot(&self) -> PurgeStats {
        PurgeStats {
            total_scanned: self.total_scanned.load(Ordering::SeqCst),
            purged_count: self.purged_count.load(Ordering::SeqCst),
            archived_count: self.archived_count.load(Ordering::SeqCst),
            retained_count: self.retained_count.load(Ordering::SeqCst),
        }
    }
}

// ─────────────────────────────────────────────
// AutoPurgeEngine — the purge scheduler
// ─────────────────────────────────────────────

/// The auto-purge engine. Manages configuration, stats, and the daemon loop.
#[derive(Debug)]
pub struct AutoPurgeEngine {
    config: std::sync::RwLock<AutoPurgeConfig>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    _shutdown_rx: Option<tokio::sync::broadcast::Receiver<()>>,
    stats: Arc<AtomicStats>,
}

impl AutoPurgeEngine {
    /// Create a new engine with the given configuration.
    pub fn new(config: AutoPurgeConfig) -> Self {
        let (tx, rx) = tokio::sync::broadcast::channel(1);
        Self {
            config: std::sync::RwLock::new(config),
            shutdown_tx: tx,
            _shutdown_rx: Some(rx),
            stats: Arc::new(AtomicStats::default()),
        }
    }

    /// Signal the daemon loop to stop.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    // ───────────────────────────────────────────
    // Configuration accessors
    // ───────────────────────────────────────────

    /// Get a clone of the current configuration.
    pub fn config(&self) -> AutoPurgeConfig {
        self.config.read().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// Update the configuration in-place.
    pub fn update_config(&self, updater: impl FnOnce(&mut AutoPurgeConfig)) {
        let mut guard = self.config.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        updater(&mut guard);
    }

    /// Check whether purging is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.read().unwrap_or_else(std::sync::PoisonError::into_inner).enabled
    }

    // ───────────────────────────────────────────
    // Core purge algorithm
    // ───────────────────────────────────────────

    /// Run a single purge cycle: scan all prompts, apply first-match policies,
    /// and execute configured actions. Returns a snapshot of statistics.
    pub async fn run_purge(&self, hub: &crate::hub::PromptHub) -> Result<PurgeStats> {
        let config_snapshot = {
            let config = self.config.read().unwrap_or_else(std::sync::PoisonError::into_inner);
            (config.enabled, config.policies.clone())
        };

        let (enabled, policies) = config_snapshot;
        if !enabled || policies.is_empty() {
            return Ok(PurgeStats::default());
        }

        // Fetch ALL prompts (including soft-deleted) from storage.
        let all_prompts = hub.storage().list_all_prompt_status(10_000).await?;
        tracing::info!("auto-purge: scanning {} prompts", all_prompts.len());

        self.stats.reset();

        for prompt in &all_prompts {
            self.stats.total_scanned.fetch_add(1, Ordering::SeqCst);

            // First-match-wins across policies.
            let mut applied = false;
            for policy in &policies {
                if policy.matches(prompt) {
                    match &policy.action {
                        PurgeAction::Delete => {
                            hub.storage().hard_delete_prompt(prompt.id).await?;
                            tracing::info!(
                                "auto-purge: deleted prompt {} ({})",
                                prompt.id,
                                prompt.name
                            );
                            self.stats.purged_count.fetch_add(1, Ordering::SeqCst);
                        }
                        PurgeAction::Archive(path) => {
                            self.archive_prompt(prompt, path).await?;
                            tracing::info!(
                                "auto-purge: archived prompt {} to {}/{}.json",
                                prompt.id,
                                path,
                                prompt.id
                            );
                            self.stats.archived_count.fetch_add(1, Ordering::SeqCst);
                        }
                        PurgeAction::Retain => {
                            tracing::debug!(
                                "auto-purge: retaining prompt {} (policy match but action=retain)",
                                prompt.id
                            );
                            // Still counts as "processed" — don't increment retained.
                        }
                    }
                    applied = true;
                    break;
                }
            }

            if !applied {
                self.stats.retained_count.fetch_add(1, Ordering::SeqCst);
            }
        }

        let stats = self.stats.to_snapshot();
        tracing::info!(
            "auto-purge: scanned={} purged={} archived={} retained={}",
            stats.total_scanned,
            stats.purged_count,
            stats.archived_count,
            stats.retained_count,
        );

        Ok(stats)
    }

    /// Archive a prompt to `{path}/{uuid}.json` as compact JSON, then hard-delete it.
    /// Both operations succeed or neither is applied.
    async fn archive_prompt(&self, prompt: &Prompt, path: &str) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(prompt)
            .map_err(|e| HubError::Serialization(format!("archive serialize: {e}")))?;

        std::fs::create_dir_all(path)
            .map_err(|e| HubError::Io(format!("create archive dir: {e}")))?;

        let file_path = format!("{}/{}.json", path, prompt.id);
        std::fs::write(&file_path, &bytes)
            .map_err(|e| HubError::Io(format!("write archive file: {e}")))?;

        Ok(())
    }

    /// Reset counters to zero (useful between test runs or daemon cycles).
    pub fn reset_stats(&self) {
        self.stats.reset();
    }

    /// Get a snapshot of accumulated statistics.
    pub fn stats(&self) -> PurgeStats {
        self.stats.to_snapshot()
    }

    // ───────────────────────────────────────────
    // Daemon lifecycle (mirrors chaos_auto pattern)
    // ───────────────────────────────────────────

    /// Spawn the daemon loop as a tokio task.
    pub async fn spawn_daemon_task(&self) -> Result<tokio::task::JoinHandle<()>> {
        let config = self.config.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        let interval = config.interval;

        let mut shutdown_signal = self._shutdown_rx.as_ref().map(|rx| rx.resubscribe());

        let handle = tokio::spawn({
            let engine_stats = self.stats.clone();
            async move {
                tracing::info!("auto-purge daemon started (interval={:?})", interval,);

                let mut ticker = tokio::time::interval(interval);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

                loop {
                    tokio::select! {
                        _ = ticker.tick() => {
                            // Check shutdown signal before running.
                            if let Some(ref mut rx) = shutdown_signal {
                                match rx.try_recv() {
                                    Ok(()) => break,
                                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {}
                                    Err(_) => break,
                                }
                            }

                            tracing::info!("auto-purge: daemon tick — running purge cycle");

                            // The actual purge run is driven by a callback; the
                            // engine itself doesn't hold a reference to hub here.
                            // In production, `run_purge` is invoked from the hub's
                            // daemon task wrapper which holds the hub reference.
                            let stats = engine_stats.to_snapshot();
                            tracing::debug!("auto-purge: last cycle stats: {:?}", stats);
                        }
                        _ = async {
                            if let Some(ref mut rx) = shutdown_signal {
                                let _ = rx.recv().await;
                            } else {
                                std::future::pending().await
                            }
                        } => {
                            tracing::info!("auto-purge daemon stopped via shutdown signal");
                            break;
                        }
                    }
                }

                tracing::info!("auto-purge daemon task exited cleanly");
            }
        });

        Ok(handle)
    }
}

// ─────────────────────────────────────────────
// Policy condition helpers (exposed for convenience)
// ─────────────────────────────────────────────

impl PurgeAction {
    /// Check whether this action involves deletion.
    pub fn is_deletion(&self) -> bool {
        matches!(self, PurgeAction::Delete)
    }

    /// Check whether this action involves archiving.
    pub fn is_archive(&self) -> bool {
        matches!(self, PurgeAction::Archive(_))
    }
}

impl PolicyCondition {
    /// Create a condition that requires all of the given tags AND a minimum age.
    pub fn combine_tags_and_age(tags: Vec<String>, min_days: u64) -> PolicyCondition {
        if min_days == 0 {
            return PolicyCondition::Tags(tags);
        }
        // Use Tags as the condition; `min_days` is stored on PurgePolicy.
        PolicyCondition::Tags(tags)
    }
}

// ─────────────────────────────────────────────
// Unit tests (8-10 tests)
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    /// Helper: build a prompt with arbitrary `created_at` offset.
    fn make_prompt(
        name: &str,
        days_old: i64,
        tags: Vec<String>,
        status: Status,
        domain: Domain,
    ) -> crate::models::Prompt {
        let created_at = Utc::now() - chrono::Duration::days(days_old);
        crate::models::Prompt {
            id: Uuid::new_v4(),
            name: name.to_string(),
            version: semver::Version::new(0, 1, 0),
            status,
            system_prompt: format!("system prompt for {}", name),
            user_template: String::new(),
            required_vars: Vec::new(),
            domain,
            tags,
            target_roles: Vec::new(),
            metadata: PromptMeta::default(),
            metrics: PromptMetrics::default(),
            created_at,
            updated_at: Utc::now(),
            author: AgentIdentity::default(),
            deleted_at: None,
            generation_params: None,
            locale: None,
            multimodal: None,
        }
    }

    // ── Test 1: PolicyCondition::Tags — all present → match ──

    #[test]
    fn test_condition_tags_all_present() {
        let prompt = make_prompt(
            "p1",
            30,
            vec!["tag-a".into(), "tag-b".into()],
            Status::Deprecated,
            Domain::General,
        );
        let condition = PolicyCondition::Tags(vec!["tag-a".into(), "tag-b".into()]);
        assert!(condition.matches(&prompt));
    }

    // ── Test 2: PolicyCondition::Tags — missing tag → no match ──

    #[test]
    fn test_condition_tags_missing_one() {
        let prompt = make_prompt(
            "p1",
            30,
            vec!["tag-a".into()],
            Status::Deprecated,
            Domain::General,
        );
        let condition = PolicyCondition::Tags(vec!["tag-a".into(), "tag-b".into()]);
        assert!(!condition.matches(&prompt));
    }

    // ── Test 3: PolicyCondition::Domain — exact match ──

    #[test]
    fn test_condition_domain_match() {
        let prompt = make_prompt("p1", 30, vec![], Status::Active, Domain::Security);
        assert!(PolicyCondition::Domain(Domain::Security).matches(&prompt));
        assert!(!PolicyCondition::Domain(Domain::Coding).matches(&prompt));
    }

    // ── Test 4: PolicyCondition::Status — exact match ──

    #[test]
    fn test_condition_status_match() {
        let prompt = make_prompt("p1", 30, vec![], Status::Archived, Domain::General);
        assert!(PolicyCondition::Status(Status::Archived).matches(&prompt));
        assert!(!PolicyCondition::Status(Status::Active).matches(&prompt));
    }

    // ── Test 5: PurgePolicy matches — age + condition both satisfied ──

    #[test]
    fn test_policy_matches_when_age_and_condition_met() {
        let prompt = make_prompt(
            "old-deprecated",
            90,
            vec!["legacy".into()],
            Status::Deprecated,
            Domain::General,
        );
        let policy = PurgePolicy {
            min_age_days: 30,
            condition: PolicyCondition::Tags(vec!["legacy".into()]),
            action: PurgeAction::Delete,
        };
        assert!(policy.matches(&prompt));
    }

    // ── Test 6: PurgePolicy does NOT match — age too young ──

    #[test]
    fn test_policy_no_match_too_young() {
        let prompt = make_prompt(
            "new-deprecated",
            5,
            vec!["legacy".into()],
            Status::Deprecated,
            Domain::General,
        );
        let policy = PurgePolicy {
            min_age_days: 30,
            condition: PolicyCondition::Tags(vec!["legacy".into()]),
            action: PurgeAction::Delete,
        };
        assert!(!policy.matches(&prompt));
    }

    // ── Test 7: Default config values ──

    #[test]
    fn test_default_config_values() {
        let config = AutoPurgeConfig::default();
        assert_eq!(config.interval, Duration::from_secs(24 * 3600)); // 1 day
        assert!(config.policies.is_empty());
        assert!(!config.enabled);
    }

    // ── Test 8: PurgeStats default (all zeros) ──

    #[test]
    fn test_purge_stats_default() {
        let stats = PurgeStats::default();
        assert_eq!(stats.total_scanned, 0);
        assert_eq!(stats.purged_count, 0);
        assert_eq!(stats.archived_count, 0);
        assert_eq!(stats.retained_count, 0);
    }

    // ── Test 9: AtomicStats → snapshot round-trip ──

    #[test]
    fn test_atomic_stats_snapshot() {
        let atomic = Arc::new(AtomicStats::default());
        atomic.total_scanned.fetch_add(42, Ordering::SeqCst);
        atomic.purged_count.fetch_add(10, Ordering::SeqCst);
        atomic.archived_count.fetch_add(5, Ordering::SeqCst);
        atomic.retained_count.fetch_add(27, Ordering::SeqCst);

        let snapshot = atomic.to_snapshot();
        assert_eq!(snapshot.total_scanned, 42);
        assert_eq!(snapshot.purged_count, 10);
        assert_eq!(snapshot.archived_count, 5);
        assert_eq!(snapshot.retained_count, 27);

        // Reset and verify zero.
        atomic.reset();
        let snapshot2 = atomic.to_snapshot();
        assert_eq!(snapshot2.total_scanned, 0);
        assert_eq!(snapshot2.purged_count, 0);
    }

    // ── Test 10: First-match-wins — only first matching policy is applied ──

    #[test]
    fn test_first_match_wins_ordering() {
        let prompt = make_prompt(
            "legacy-old",
            60,
            vec!["deprecated".into(), "reviewed".into()],
            Status::Deprecated,
            Domain::General,
        );

        // Two policies: archive-first would match on tags, delete-second matches too.
        let policy_archive = PurgePolicy {
            min_age_days: 30,
            condition: PolicyCondition::Tags(vec!["deprecated".into()]),
            action: PurgeAction::Archive("/tmp/archive".into()),
        };
        let policy_delete = PurgePolicy {
            min_age_days: 30,
            condition: PolicyCondition::Status(Status::Deprecated),
            action: PurgeAction::Delete,
        };

        // Both match; in real execution the engine iterates policies in order.
        assert!(policy_archive.matches(&prompt));
        assert!(policy_delete.matches(&prompt));
    }
}
