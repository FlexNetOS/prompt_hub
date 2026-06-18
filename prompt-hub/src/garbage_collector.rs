#![forbid(unsafe_code)]

use crate::error::Result;
use crate::retention::{DataType, RetentionPolicy};
use crate::storage::Storage;
use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, instrument};

/// Soft-delete garbage collector.
///
/// Periodically purges soft-deleted prompts older than the retention period,
/// cleans orphaned embeddings, and vacuums the database for storage efficiency.
#[derive(Debug)]
pub struct GarbageCollector {
    retention_policy: RetentionPolicy,
    prompts_purged: AtomicU64,
    embeddings_cleaned: AtomicU64,
    vacuums_run: AtomicU64,
    total_errors: AtomicU64,
    enabled: std::sync::atomic::AtomicBool,
}

/// Report from a garbage collection run.
#[derive(Debug, Clone)]
pub struct GcReport {
    pub prompts_purged: u64,
    pub embeddings_cleaned: u64,
    pub vacuum_performed: bool,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

/// Configuration for the garbage collector.
#[derive(Debug, Clone)]
pub struct GcConfig {
    pub enabled: bool,
    pub retention_days_soft_deleted: u32,
    pub retention_days_orphaned_embeddings: u32,
    pub vacuum_enabled: bool,
    pub dry_run: bool,
}

impl GarbageCollector {
    /// Create a new garbage collector.
    pub fn new(retention_policy: RetentionPolicy) -> Self {
        Self {
            retention_policy,
            prompts_purged: AtomicU64::new(0),
            embeddings_cleaned: AtomicU64::new(0),
            vacuums_run: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            enabled: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// Run a full garbage collection cycle against the live storage handle.
    ///
    /// Phases run in order: purge expired soft-deleted prompts, clean orphaned
    /// embedding rows, then (if the retention policy requests it) vacuum to
    /// reclaim freed pages. Each phase executes real, transactional `DELETE`s
    /// through [`Storage`] — the same handle and transactional idiom the
    /// `auto-purge` path uses via `hard_delete_prompt`.
    #[instrument(skip(self, storage))]
    pub async fn collect(&self, storage: &Storage) -> Result<GcReport> {
        if !self.enabled.load(Ordering::SeqCst) {
            return Ok(GcReport {
                prompts_purged: 0,
                embeddings_cleaned: 0,
                vacuum_performed: false,
                errors: vec!["GC is disabled".to_string()],
                duration_ms: 0,
            });
        }

        let start = std::time::Instant::now();
        let errors = Vec::new();

        // Phase 1: Purge soft-deleted prompts past their retention window.
        let purged = self.purge_soft_deleted(storage).await?;

        // Phase 2: Clean orphaned embeddings (rows with no owning prompt).
        let cleaned = self.clean_orphaned_embeddings(storage).await?;

        // Phase 3: Vacuum if the retention policy requests it.
        let vacuumed = if self.retention_policy.auto_purge_enabled() {
            self.vacuum(storage).await?;
            true
        } else {
            false
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        info!(
            "GC complete: {} prompts purged, {} embeddings cleaned, vacuum={}",
            purged, cleaned, vacuumed
        );

        Ok(GcReport {
            prompts_purged: purged,
            embeddings_cleaned: cleaned,
            vacuum_performed: vacuumed,
            errors,
            duration_ms,
        })
    }

    /// Purge soft-deleted prompts older than the configured retention window.
    ///
    /// Deletes every prompt whose `deleted_at` is older than
    /// `now - retention_days(SoftDeletedPrompt)` and returns the real number of
    /// rows removed. Cascades to each prompt's embedding via the foreign key.
    #[instrument(skip(self, storage))]
    pub async fn purge_soft_deleted(&self, storage: &Storage) -> Result<u64> {
        let retention_days = self
            .retention_policy
            .get_period(&DataType::SoftDeletedPrompt);
        let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
        info!(
            "Purging soft-deleted prompts older than {} days (cutoff {})",
            retention_days, cutoff
        );

        let purged = storage.purge_soft_deleted(cutoff).await?;
        self.prompts_purged.fetch_add(purged, Ordering::SeqCst);
        Ok(purged)
    }

    /// Clean up embedding rows that no longer reference an existing prompt.
    ///
    /// Returns the real number of orphaned embedding rows removed.
    #[instrument(skip(self, storage))]
    pub async fn clean_orphaned_embeddings(&self, storage: &Storage) -> Result<u64> {
        info!("Cleaning orphaned embeddings (rows with no owning prompt)");

        let cleaned = storage.delete_orphaned_embeddings().await?;
        self.embeddings_cleaned.fetch_add(cleaned, Ordering::SeqCst);
        Ok(cleaned)
    }

    /// Vacuum the database to reclaim storage freed by deleted rows.
    #[instrument(skip(self, storage))]
    pub async fn vacuum(&self, storage: &Storage) -> Result<()> {
        info!("Running database vacuum");
        storage.vacuum().await?;
        self.vacuums_run.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// Get cumulative statistics.
    pub fn stats(&self) -> GcStats {
        GcStats {
            prompts_purged_total: self.prompts_purged.load(Ordering::SeqCst),
            embeddings_cleaned_total: self.embeddings_cleaned.load(Ordering::SeqCst),
            vacuums_run_total: self.vacuums_run.load(Ordering::SeqCst),
            total_errors: self.total_errors.load(Ordering::SeqCst),
        }
    }

    /// Enable or disable the collector.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    /// Check if collector is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Load configuration.
    pub fn configure(&self, config: &GcConfig) {
        self.set_enabled(config.enabled);
        info!(
            "GC configured: enabled={}, dry_run={}",
            config.enabled, config.dry_run
        );
    }
}

/// Cumulative GC statistics.
#[derive(Debug, Clone)]
pub struct GcStats {
    pub prompts_purged_total: u64,
    pub embeddings_cleaned_total: u64,
    pub vacuums_run_total: u64,
    pub total_errors: u64,
}

impl Default for GarbageCollector {
    fn default() -> Self {
        Self::new(RetentionPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Prompt;
    use crate::storage::{Storage, StorageConfig};
    use libsql::params;
    use uuid::Uuid;

    /// Build an in-memory storage instance with all migrations applied.
    async fn in_memory_storage() -> Storage {
        let config = StorageConfig {
            db_path: ":memory:".to_string(),
            max_connections: 2,
            ..Default::default()
        };
        Storage::new(config)
            .await
            .expect("create in-memory storage")
    }

    /// A GC whose soft-deleted-prompt retention window is 30 days (default).
    fn default_gc() -> GarbageCollector {
        GarbageCollector::default()
    }

    /// Soft-delete `prompt`, backdating `deleted_at` by `days_ago` days so the
    /// retention cutoff can be exercised deterministically.
    async fn soft_delete_backdated(storage: &Storage, prompt: &Prompt, days_ago: i64) {
        storage.delete_prompt(prompt.id).await.expect("soft delete");
        let backdated = (Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339();
        let conn = storage.acquire().await.expect("acquire");
        conn.execute(
            "UPDATE prompts SET deleted_at = ?1 WHERE id = ?2;",
            params!(backdated, prompt.id.to_string()),
        )
        .await
        .expect("backdate deleted_at");
    }

    /// Count rows in `prompts` (regardless of soft-delete status).
    async fn count_all_prompts(storage: &Storage) -> i64 {
        let conn = storage.acquire().await.expect("acquire");
        let mut rows = conn
            .query("SELECT COUNT(*) FROM prompts;", params!())
            .await
            .expect("count prompts");
        let row = rows.next().await.expect("row").expect("some row");
        row.get(0).expect("count")
    }

    /// Count rows in `embeddings`.
    async fn count_embeddings(storage: &Storage) -> i64 {
        let conn = storage.acquire().await.expect("acquire");
        let mut rows = conn
            .query("SELECT COUNT(*) FROM embeddings;", params!())
            .await
            .expect("count embeddings");
        let row = rows.next().await.expect("row").expect("some row");
        row.get(0).expect("count")
    }

    /// Insert an embedding row directly with an arbitrary `prompt_id`. When
    /// `prompt_id` references no prompt this seeds an orphan; foreign-key
    /// enforcement is dropped for the insert so the orphan can exist.
    async fn insert_embedding_raw(storage: &Storage, prompt_id: Uuid) {
        let conn = storage.acquire().await.expect("acquire");
        conn.execute("PRAGMA foreign_keys = OFF;", params!())
            .await
            .expect("fk off");
        conn.execute(
            "INSERT INTO embeddings (prompt_id, embedding) VALUES (?1, ?2);",
            params!(prompt_id.to_string(), vec![0u8; 4]),
        )
        .await
        .expect("insert embedding");
        conn.execute("PRAGMA foreign_keys = ON;", params!())
            .await
            .expect("fk on");
    }

    #[test]
    fn test_gc_new() {
        let policy = RetentionPolicy::default();
        let gc = GarbageCollector::new(policy);
        assert!(gc.is_enabled());
    }

    #[tokio::test]
    async fn test_gc_disabled() {
        let storage = in_memory_storage().await;
        let policy = RetentionPolicy::default();
        let gc = GarbageCollector::new(policy);
        gc.set_enabled(false);
        let report = gc.collect(&storage).await.unwrap();
        assert!(!report.errors.is_empty());
        assert_eq!(report.prompts_purged, 0);
    }

    #[tokio::test]
    async fn test_gc_collect_enabled_empty_store() {
        let storage = in_memory_storage().await;
        let gc = default_gc();
        let report = gc.collect(&storage).await.unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(report.prompts_purged, 0);
        assert_eq!(report.embeddings_cleaned, 0);
        assert!(report.vacuum_performed);
    }

    /// Real deletion: an expired soft-deleted prompt is permanently removed and
    /// counted; a freshly soft-deleted prompt within the window is preserved.
    #[tokio::test]
    async fn test_purge_soft_deleted_real_deletion() {
        let storage = in_memory_storage().await;
        let gc = default_gc();

        // Expired: soft-deleted 60 days ago (> 30-day default retention).
        let expired = Prompt::new("expired", "You are helpful.");
        storage.insert_prompt(&expired).await.unwrap();
        soft_delete_backdated(&storage, &expired, 60).await;

        // Recent: soft-deleted 1 day ago (within retention window — keep).
        let recent = Prompt::new("recent", "You are helpful.");
        storage.insert_prompt(&recent).await.unwrap();
        soft_delete_backdated(&storage, &recent, 1).await;

        // Active: never deleted — must survive.
        let active = Prompt::new("active", "You are helpful.");
        storage.insert_prompt(&active).await.unwrap();

        assert_eq!(count_all_prompts(&storage).await, 3);

        let purged = gc.purge_soft_deleted(&storage).await.unwrap();
        assert_eq!(purged, 1, "exactly the expired prompt should be purged");

        // The expired row is physically gone; recent + active remain.
        assert_eq!(count_all_prompts(&storage).await, 2);
        assert!(storage.get_prompt(expired.id).await.unwrap().is_none());
        assert!(storage.get_prompt(active.id).await.unwrap().is_some());

        // Cumulative counter reflects the real deletion.
        assert_eq!(gc.stats().prompts_purged_total, 1);
    }

    /// Real deletion: orphaned embedding rows are removed and counted; an
    /// embedding owned by a live prompt is preserved.
    #[tokio::test]
    async fn test_clean_orphaned_embeddings_real_deletion() {
        let storage = in_memory_storage().await;
        let gc = default_gc();

        // Live prompt + its embedding (owned, must survive).
        let owner = Prompt::new("owner", "You are helpful.");
        storage.insert_prompt(&owner).await.unwrap();
        storage.upsert_embedding(owner.id, &[0u8; 4]).await.unwrap();

        // Two orphan embeddings referencing non-existent prompts.
        insert_embedding_raw(&storage, Uuid::new_v4()).await;
        insert_embedding_raw(&storage, Uuid::new_v4()).await;

        assert_eq!(count_embeddings(&storage).await, 3);

        let cleaned = gc.clean_orphaned_embeddings(&storage).await.unwrap();
        assert_eq!(cleaned, 2, "both orphan embeddings should be cleaned");

        // Only the owned embedding remains.
        assert_eq!(count_embeddings(&storage).await, 1);
        assert_eq!(gc.stats().embeddings_cleaned_total, 2);
    }

    /// End-to-end: a full collect cycle physically removes expired soft-deleted
    /// prompts AND orphaned embeddings, with real counts in the report.
    #[tokio::test]
    async fn test_collect_full_cycle_real_deletion() {
        let storage = in_memory_storage().await;
        let gc = default_gc();

        let expired = Prompt::new("expired", "You are helpful.");
        storage.insert_prompt(&expired).await.unwrap();
        soft_delete_backdated(&storage, &expired, 90).await;

        insert_embedding_raw(&storage, Uuid::new_v4()).await;

        let report = gc.collect(&storage).await.unwrap();
        assert!(report.errors.is_empty());
        assert_eq!(report.prompts_purged, 1);
        assert_eq!(report.embeddings_cleaned, 1);
        assert!(report.vacuum_performed);

        assert_eq!(count_all_prompts(&storage).await, 0);
        assert_eq!(count_embeddings(&storage).await, 0);
    }

    /// Soft-delete cascades to the owned embedding via the FK, then the GC
    /// purges the prompt — proving the GC never leaves a dangling embedding.
    #[tokio::test]
    async fn test_purge_cascades_to_embedding() {
        let storage = in_memory_storage().await;
        let gc = default_gc();

        let prompt = Prompt::new("with-embedding", "You are helpful.");
        storage.insert_prompt(&prompt).await.unwrap();
        storage
            .upsert_embedding(prompt.id, &[1u8; 4])
            .await
            .unwrap();
        assert_eq!(count_embeddings(&storage).await, 1);

        soft_delete_backdated(&storage, &prompt, 45).await;
        let purged = gc.purge_soft_deleted(&storage).await.unwrap();
        assert_eq!(purged, 1);

        // The embedding was removed by ON DELETE CASCADE — no orphan left.
        assert_eq!(count_embeddings(&storage).await, 0);
        let cleaned = gc.clean_orphaned_embeddings(&storage).await.unwrap();
        assert_eq!(cleaned, 0);
    }

    #[tokio::test]
    async fn test_vacuum() {
        let storage = in_memory_storage().await;
        let gc = default_gc();
        assert!(gc.vacuum(&storage).await.is_ok());
        assert_eq!(gc.stats().vacuums_run_total, 1);
    }

    #[test]
    fn test_stats() {
        let gc = GarbageCollector::default();
        let stats = gc.stats();
        assert_eq!(stats.prompts_purged_total, 0);
        assert_eq!(stats.vacuums_run_total, 0);
    }

    #[test]
    fn test_configure() {
        let gc = GarbageCollector::default();
        gc.configure(&GcConfig {
            enabled: false,
            retention_days_soft_deleted: 15,
            retention_days_orphaned_embeddings: 90,
            vacuum_enabled: true,
            dry_run: false,
        });
        assert!(!gc.is_enabled());
    }

    #[test]
    fn test_default() {
        let gc: GarbageCollector = Default::default();
        assert!(gc.is_enabled());
    }

    #[test]
    fn test_gc_report_clone() {
        let report = GcReport {
            prompts_purged: 5,
            embeddings_cleaned: 3,
            vacuum_performed: true,
            errors: vec![],
            duration_ms: 100,
        };
        let cloned = report.clone();
        assert_eq!(cloned.prompts_purged, 5);
    }

    #[test]
    fn test_gc_stats_clone() {
        let stats = GcStats {
            prompts_purged_total: 10,
            embeddings_cleaned_total: 5,
            vacuums_run_total: 2,
            total_errors: 0,
        };
        let cloned = stats.clone();
        assert_eq!(cloned.prompts_purged_total, 10);
    }
}
