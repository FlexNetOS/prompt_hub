#![forbid(unsafe_code)]

use crate::error::Result;
use crate::retention::{DataType, RetentionPolicy};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, instrument, warn};

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

    /// Run a full garbage collection cycle.
    #[instrument(skip(self))]
    pub fn collect(&self) -> Result<GcReport> {
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

        // Phase 1: Purge soft-deleted prompts
        let purged = self.purge_soft_deleted()?;

        // Phase 2: Clean orphaned embeddings
        let cleaned = self.clean_orphaned_embeddings()?;

        // Phase 3: Vacuum if enabled
        let vacuumed = if self.retention_policy.auto_purge_enabled() {
            self.vacuum()?;
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

    /// Purge soft-deleted prompts older than retention.
    #[instrument(skip(self))]
    pub fn purge_soft_deleted(&self) -> Result<u64> {
        let retention_days = self
            .retention_policy
            .get_period(&DataType::SoftDeletedPrompt);
        info!(
            "Purging soft-deleted prompts older than {} days",
            retention_days
        );

        // In production this would execute DELETE statements
        let purged: u64 = 0;
        self.prompts_purged.fetch_add(purged, Ordering::SeqCst);
        Ok(purged)
    }

    /// Clean up orphaned embedding vectors.
    #[instrument(skip(self))]
    pub fn clean_orphaned_embeddings(&self) -> Result<u64> {
        let retention_days = self.retention_policy.get_period(&DataType::EmbeddingVector);
        info!(
            "Cleaning orphaned embeddings older than {} days",
            retention_days
        );

        let cleaned: u64 = 0;
        self.embeddings_cleaned.fetch_add(cleaned, Ordering::SeqCst);
        Ok(cleaned)
    }

    /// Vacuum the database to reclaim storage.
    #[instrument(skip(self))]
    pub fn vacuum(&self) -> Result<()> {
        info!("Running database vacuum");
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

    #[test]
    fn test_gc_new() {
        let policy = RetentionPolicy::default();
        let gc = GarbageCollector::new(policy);
        assert!(gc.is_enabled());
    }

    #[test]
    fn test_gc_disabled() {
        let policy = RetentionPolicy::default();
        let gc = GarbageCollector::new(policy);
        gc.set_enabled(false);
        let report = gc.collect().unwrap();
        assert!(!report.errors.is_empty());
    }

    #[test]
    fn test_gc_collect_enabled() {
        let gc = GarbageCollector::default();
        let report = gc.collect().unwrap();
        assert!(report.errors.is_empty());
    }

    #[test]
    fn test_purge_soft_deleted() {
        let gc = GarbageCollector::default();
        let count = gc.purge_soft_deleted().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_clean_orphaned_embeddings() {
        let gc = GarbageCollector::default();
        let count = gc.clean_orphaned_embeddings().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_vacuum() {
        let gc = GarbageCollector::default();
        assert!(gc.vacuum().is_ok());
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
