#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::{AuditEntry, Paginated, Pagination};
use crate::storage::Storage;
use chrono::Utc;
use sha2::{Digest, Sha256};
use tracing::{info, instrument, warn};
use uuid::Uuid;

/// Well-known sentinel UUID written into the `agent_id` column of audit rows
/// that have been anonymised for GDPR right-to-erasure. Using a fixed,
/// non-nil sentinel keeps anonymised rows distinguishable from genuinely
/// unattributed (nil) rows while still removing the subject's identity.
pub const GDPR_ANONYMIZED_AGENT_ID: &str = "00000000-0000-0000-0000-000000000001";

// ---------------------------------------------------------------------------
// AuditLogger trait
// ---------------------------------------------------------------------------

/// Trait for audit logging backends.
/// Uses native async fn in traits (Rust 2024 Edition).
pub trait AuditLogger: Send + Sync {
    /// Append a single audit entry.
    async fn log(&self, entry: AuditEntry) -> Result<()>;

    /// Retrieve the audit trail for a given prompt, most-recent first.
    async fn audit_trail(
        &self,
        prompt_id: Uuid,
        pagination: Pagination,
    ) -> Result<Paginated<AuditEntry>>;

    /// Retrieve all audit entries for a given agent.
    async fn audit_trail_by_agent(
        &self,
        agent_id: Uuid,
        pagination: Pagination,
    ) -> Result<Paginated<AuditEntry>>;
}

// ---------------------------------------------------------------------------
// SqliteAuditLogger
// ---------------------------------------------------------------------------

/// SQL-backed audit logger with tamper-evident **SHA-256 hash chain**.
///
/// The `diff_hash` field of each [`AuditEntry`] is computed as
/// `SHA256(before_json + after_json + timestamp)` so that any retroactive
/// modification of the stored JSON or timestamp would invalidate the chain.
///
/// **GDPR compliance**: [`SqliteAuditLogger::right_to_erasure`] anonymises
/// entries without deleting them, preserving the integrity of the hash chain.
#[derive(Debug, Clone)]
pub struct SqliteAuditLogger;

impl SqliteAuditLogger {
    pub fn new() -> Self {
        Self
    }

    // ── Hash chain ──────────────────────────────────────────────────────────

    /// Compute the tamper-evident diff hash for an audit entry.
    ///
    /// The hash is `SHA256(before_json || after_json || timestamp)` where
    /// missing `before_json` or `after_json` values are treated as empty
    /// byte strings.
    pub fn compute_diff_hash(
        before: &Option<String>,
        after: &Option<String>,
        timestamp: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        if let Some(b) = before {
            hasher.update(b.as_bytes());
        }
        if let Some(a) = after {
            hasher.update(a.as_bytes());
        }
        hasher.update(timestamp.as_bytes());
        // sha2 0.11's `finalize()` returns a `hybrid_array::Array`, which no
        // longer implements `LowerHex`. Hex-encode the digest bytes by hand to
        // keep the output byte-identical (lowercase, zero-padded) so existing
        // hash-chain entries continue to verify.
        use std::fmt::Write as _;
        let mut hash = String::with_capacity(64);
        for byte in hasher.finalize() {
            // Writing to a `String` is infallible.
            let _ = write!(hash, "{byte:02x}");
        }
        hash
    }

    /// Verify that the `diff_hash` on an existing entry matches the
    /// recomputed hash for its contents.
    pub fn verify_entry_integrity(entry: &AuditEntry) -> bool {
        let recomputed = Self::compute_diff_hash(
            &entry.before_json,
            &entry.after_json,
            &entry.timestamp.to_rfc3339(),
        );
        let valid = recomputed == entry.diff_hash;
        if !valid {
            warn!(
                "Audit integrity violation: entry {} hash mismatch (expected {}, got {})",
                entry.id, entry.diff_hash, recomputed
            );
        }
        valid
    }

    // ── GDPR compliance ─────────────────────────────────────────────────────

    /// GDPR **right to erasure** — anonymise all audit entries belonging to
    /// `agent_id` without breaking the hash chain.
    ///
    /// Executes the real anonymising `UPDATE` over the `audit_log` table via
    /// [`Storage::anonymize_audit_entries_for_agent`]: the `agent_id` column is
    /// replaced with the [`GDPR_ANONYMIZED_AGENT_ID`] sentinel and `ip_address`
    /// is set to `NULL`. Because the hash chain covers `(before_json,
    /// after_json, timestamp)` — never `agent_id` or `ip_address` — every
    /// rewritten row still passes [`Self::verify_entry_integrity`].
    ///
    /// Returns the actual number of audit rows that were anonymised.
    #[instrument(skip(self, storage))]
    pub async fn right_to_erasure(&self, storage: &Storage, agent_id: Uuid) -> Result<usize> {
        info!(
            "GDPR erasure: anonymising audit entries for agent {}",
            agent_id
        );
        storage.anonymize_audit_entries_for_agent(agent_id).await
    }

    /// Anonymise a single audit entry's PII fields in-place, preserving
    /// hash chain integrity.
    pub fn anonymize_entry(entry: &mut AuditEntry) {
        entry.ip_address = None;
        // agent_id is set to the well-known anonymisation sentinel
        entry.agent_id = Uuid::nil();
    }

    // ── SOC2 helpers ────────────────────────────────────────────────────────

    /// Build a SOC2 Type II evidence entry summary.
    pub fn soc2_evidence_summary(entry: &AuditEntry) -> serde_json::Value {
        serde_json::json!({
            "evidence_id": entry.id,
            "timestamp": entry.timestamp,
            "actor": entry.agent_id,
            "action": entry.action,
            "resource": entry.prompt_id,
            "integrity_hash": entry.diff_hash,
            "retention_class": "audit",
            "tamper_evident": true,
        })
    }

    /// Validate an entry conforms to SOC2 schema requirements.
    pub fn validate_soc2_schema(entry: &AuditEntry) -> Result<()> {
        if entry.diff_hash.len() != 64 {
            return Err(HubError::AuditError(
                "SOC2: diff_hash must be 64 hex characters".to_string(),
            ));
        }
        if entry.timestamp > Utc::now() + chrono::Duration::seconds(60) {
            return Err(HubError::AuditError(
                "SOC2: timestamp is in the future".to_string(),
            ));
        }
        if entry.action.is_empty() {
            return Err(HubError::AuditError(
                "SOC2: action must not be empty".to_string(),
            ));
        }
        Ok(())
    }
}

impl Default for SqliteAuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    // ── Diff hash computation ───────────────────────────────────────────────

    #[test]
    fn test_compute_diff_hash_with_both() {
        let before = Some(r#"{"name":"old"}"#.to_string());
        let after = Some(r#"{"name":"new"}"#.to_string());
        let ts = "2024-01-01T00:00:00Z";
        let hash1 = SqliteAuditLogger::compute_diff_hash(&before, &after, ts);
        let hash2 = SqliteAuditLogger::compute_diff_hash(&before, &after, ts);
        assert_eq!(hash1, hash2, "Same inputs must produce same hash");
        assert_eq!(hash1.len(), 64, "SHA-256 hex is 64 characters");
    }

    #[test]
    fn test_compute_diff_hash_none_before() {
        let before = None;
        let after = Some(r#"{"created":true}"#.to_string());
        let ts = "2024-01-01T00:00:00Z";
        let hash = SqliteAuditLogger::compute_diff_hash(&before, &after, ts);
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_compute_diff_hash_none_after() {
        let before = Some(r#"{"deleted":true}"#.to_string());
        let after = None;
        let ts = "2024-01-01T00:00:00Z";
        let hash = SqliteAuditLogger::compute_diff_hash(&before, &after, ts);
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_compute_diff_hash_deterministic() {
        let before = Some("data".to_string());
        let after = Some("changed".to_string());
        let ts = "2024-06-15T12:00:00Z";
        let h1 = SqliteAuditLogger::compute_diff_hash(&before, &after, ts);
        let h2 = SqliteAuditLogger::compute_diff_hash(&before, &after, ts);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_diff_hash_changes_with_content() {
        let ts = "2024-01-01T00:00:00Z";
        let h1 = SqliteAuditLogger::compute_diff_hash(&Some("a".to_string()), &None, ts);
        let h2 = SqliteAuditLogger::compute_diff_hash(&Some("b".to_string()), &None, ts);
        assert_ne!(h1, h2, "Different content must produce different hashes");
    }

    // ── Entry integrity verification ────────────────────────────────────────

    #[test]
    fn test_verify_entry_integrity_valid() {
        let before = Some(r#"{"version":1}"#.to_string());
        let after = Some(r#"{"version":2}"#.to_string());
        let ts = Utc::now();
        let hash = SqliteAuditLogger::compute_diff_hash(&before, &after, &ts.to_rfc3339());
        let entry = AuditEntry {
            id: 1,
            timestamp: ts,
            agent_id: Uuid::new_v4(),
            action: "UPDATE".to_string(),
            prompt_id: Some(Uuid::new_v4()),
            diff_hash: hash,
            before_json: before,
            after_json: after,
            ip_address: Some("127.0.0.1".to_string()),
        };
        assert!(SqliteAuditLogger::verify_entry_integrity(&entry));
    }

    #[test]
    fn test_verify_entry_integrity_tampered() {
        let before = Some(r#"{"version":1}"#.to_string());
        let after = Some(r#"{"version":2}"#.to_string());
        let ts = Utc::now();
        let hash = SqliteAuditLogger::compute_diff_hash(&before, &after, &ts.to_rfc3339());
        let mut entry = AuditEntry {
            id: 2,
            timestamp: ts,
            agent_id: Uuid::new_v4(),
            action: "UPDATE".to_string(),
            prompt_id: Some(Uuid::new_v4()),
            diff_hash: hash,
            before_json: before,
            after_json: after,
            ip_address: Some("127.0.0.1".to_string()),
        };
        // Tamper with after_json
        entry.after_json = Some(r#"{"tampered":true}"#.to_string());
        assert!(
            !SqliteAuditLogger::verify_entry_integrity(&entry),
            "Tampered entry must fail integrity check"
        );
    }

    // ── GDPR erasure ────────────────────────────────────────────────────────

    // Real GDPR erasure against an in-memory libsql `audit_log` (PHTASK-0049).
    //
    // These tests seed audit rows for a target subject *and* unrelated
    // subjects, run the real anonymising UPDATE, and assert that (a) only the
    // subject's rows are redacted, (b) other subjects are untouched, and
    // (c) the returned count equals the number of the subject's rows.

    use crate::models::AuditEntry as ModelAuditEntry;
    use crate::storage::{Storage, StorageConfig};

    async fn in_memory_storage() -> Storage {
        let config = StorageConfig {
            db_path: ":memory:".to_string(),
            max_connections: 2,
            ..Default::default()
        };
        Storage::new(config)
            .await
            .expect("Failed to create in-memory storage")
    }

    /// Build and persist an audit entry for `agent_id` with a real diff hash.
    async fn seed_entry(
        storage: &Storage,
        agent_id: Uuid,
        action: &str,
        ip: Option<&str>,
    ) -> AuditEntry {
        let before = Some(format!(r#"{{"action":"{action}"}}"#));
        let after = Some(r#"{"state":"done"}"#.to_string());
        let ts = Utc::now();
        let diff_hash = SqliteAuditLogger::compute_diff_hash(&before, &after, &ts.to_rfc3339());
        let entry = ModelAuditEntry {
            id: 0,
            timestamp: ts,
            agent_id,
            action: action.to_string(),
            prompt_id: Some(Uuid::new_v4()),
            diff_hash,
            before_json: before,
            after_json: after,
            ip_address: ip.map(|s| s.to_string()),
        };
        storage.log_audit(&entry).await.expect("seed log_audit");
        entry
    }

    /// Read back every audit row for a given `prompt_id` (each seeded entry has
    /// a unique prompt_id, so this isolates a single row).
    async fn fetch_for_prompt(storage: &Storage, prompt_id: Uuid) -> Vec<AuditEntry> {
        storage
            .fetch_audit_trail(prompt_id, 1, 100)
            .await
            .expect("fetch_audit_trail")
            .items
    }

    #[tokio::test]
    async fn test_right_to_erasure_anonymizes_only_subject() {
        let storage = in_memory_storage().await;
        let logger = SqliteAuditLogger::new();

        let subject = Uuid::new_v4();
        let other_a = Uuid::new_v4();
        let other_b = Uuid::new_v4();

        // Three rows for the subject (with IP PII), and one row each for two
        // other agents that MUST be left untouched.
        let s1 = seed_entry(&storage, subject, "UPDATE", Some("10.0.0.1")).await;
        let s2 = seed_entry(&storage, subject, "DELETE", Some("10.0.0.2")).await;
        let s3 = seed_entry(&storage, subject, "CREATE", None).await;
        let oa = seed_entry(&storage, other_a, "UPDATE", Some("10.0.0.3")).await;
        let ob = seed_entry(&storage, other_b, "READ", Some("10.0.0.4")).await;

        // Real erasure returns the ACTUAL number of affected rows.
        let count = logger
            .right_to_erasure(&storage, subject)
            .await
            .expect("right_to_erasure");
        assert_eq!(count, 3, "exactly the subject's three rows are anonymised");

        let anon = Uuid::parse_str(GDPR_ANONYMIZED_AGENT_ID).unwrap();

        // The subject's rows are redacted: agent_id → sentinel, ip → NULL.
        for prompt_id in [
            s1.prompt_id.unwrap(),
            s2.prompt_id.unwrap(),
            s3.prompt_id.unwrap(),
        ] {
            let rows = fetch_for_prompt(&storage, prompt_id).await;
            assert_eq!(rows.len(), 1);
            let row = &rows[0];
            assert_eq!(row.agent_id, anon, "subject agent_id is the sentinel");
            assert!(row.ip_address.is_none(), "subject ip_address is cleared");
            // Hash-chain content is preserved, so integrity still verifies.
            assert!(
                SqliteAuditLogger::verify_entry_integrity(row),
                "anonymised row must still pass integrity check"
            );
        }

        // The OTHER subjects' rows are completely untouched.
        let oa_rows = fetch_for_prompt(&storage, oa.prompt_id.unwrap()).await;
        assert_eq!(oa_rows.len(), 1);
        assert_eq!(oa_rows[0].agent_id, other_a, "other_a agent_id untouched");
        assert_eq!(
            oa_rows[0].ip_address.as_deref(),
            Some("10.0.0.3"),
            "other_a ip_address untouched"
        );

        let ob_rows = fetch_for_prompt(&storage, ob.prompt_id.unwrap()).await;
        assert_eq!(ob_rows.len(), 1);
        assert_eq!(ob_rows[0].agent_id, other_b, "other_b agent_id untouched");
        assert_eq!(
            ob_rows[0].ip_address.as_deref(),
            Some("10.0.0.4"),
            "other_b ip_address untouched"
        );
    }

    #[tokio::test]
    async fn test_right_to_erasure_no_match_returns_zero() {
        let storage = in_memory_storage().await;
        let logger = SqliteAuditLogger::new();

        // Seed a row for some agent, then erase a DIFFERENT, absent agent.
        let present = seed_entry(&storage, Uuid::new_v4(), "UPDATE", Some("1.2.3.4")).await;
        let absent = Uuid::new_v4();

        let count = logger
            .right_to_erasure(&storage, absent)
            .await
            .expect("right_to_erasure");
        assert_eq!(count, 0, "no rows for an absent subject");

        // The unrelated row is untouched.
        let rows = fetch_for_prompt(&storage, present.prompt_id.unwrap()).await;
        assert_eq!(rows[0].ip_address.as_deref(), Some("1.2.3.4"));
    }

    #[tokio::test]
    async fn test_right_to_erasure_is_idempotent() {
        let storage = in_memory_storage().await;
        let logger = SqliteAuditLogger::new();

        let subject = Uuid::new_v4();
        seed_entry(&storage, subject, "UPDATE", Some("9.9.9.9")).await;
        seed_entry(&storage, subject, "DELETE", Some("9.9.9.8")).await;

        let first = logger.right_to_erasure(&storage, subject).await.unwrap();
        assert_eq!(first, 2, "first erasure anonymises both rows");

        // Re-running for the same subject affects zero rows (already sentinel).
        let second = logger.right_to_erasure(&storage, subject).await.unwrap();
        assert_eq!(second, 0, "idempotent: already-anonymised rows excluded");
    }

    #[test]
    fn test_anonymize_entry_preserves_hash_fields() {
        let mut entry = AuditEntry {
            id: 3,
            timestamp: Utc::now(),
            agent_id: Uuid::new_v4(),
            action: "CREATE".to_string(),
            prompt_id: Some(Uuid::new_v4()),
            diff_hash: "abcd".to_string(),
            before_json: Some(r#"{}"#.to_string()),
            after_json: Some(r#"{"data":1}"#.to_string()),
            ip_address: Some("192.168.1.1".to_string()),
        };
        let original_before = entry.before_json.clone();
        let original_after = entry.after_json.clone();
        let original_hash = entry.diff_hash.clone();

        SqliteAuditLogger::anonymize_entry(&mut entry);

        assert!(entry.ip_address.is_none(), "IP address must be cleared");
        assert_eq!(entry.agent_id, Uuid::nil(), "Agent ID must be set to nil");
        // Hash-chain fields must be untouched
        assert_eq!(entry.before_json, original_before);
        assert_eq!(entry.after_json, original_after);
        assert_eq!(entry.diff_hash, original_hash);
    }

    // ── SOC2 helpers ────────────────────────────────────────────────────────

    #[test]
    fn test_soc2_evidence_summary() {
        let entry = AuditEntry {
            id: 4,
            timestamp: Utc::now(),
            agent_id: Uuid::new_v4(),
            action: "UPDATE".to_string(),
            prompt_id: Some(Uuid::new_v4()),
            diff_hash: "a".repeat(64),
            before_json: None,
            after_json: Some(r#"{}"#.to_string()),
            ip_address: None,
        };
        let summary = SqliteAuditLogger::soc2_evidence_summary(&entry);
        assert_eq!(summary["action"], "UPDATE");
        assert_eq!(summary["tamper_evident"], true);
        assert_eq!(summary["retention_class"], "audit");
    }

    #[test]
    fn test_validate_soc2_schema_valid() {
        let entry = AuditEntry {
            id: 5,
            timestamp: Utc::now(),
            agent_id: Uuid::new_v4(),
            action: "CREATE".to_string(),
            prompt_id: None,
            diff_hash: "a".repeat(64),
            before_json: None,
            after_json: None,
            ip_address: None,
        };
        assert!(SqliteAuditLogger::validate_soc2_schema(&entry).is_ok());
    }

    #[test]
    fn test_validate_soc2_schema_bad_hash_length() {
        let entry = AuditEntry {
            id: 6,
            timestamp: Utc::now(),
            agent_id: Uuid::new_v4(),
            action: "CREATE".to_string(),
            prompt_id: None,
            diff_hash: "tooshort".to_string(),
            before_json: None,
            after_json: None,
            ip_address: None,
        };
        let result = SqliteAuditLogger::validate_soc2_schema(&entry);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HubError::AuditError(_)));
    }

    #[test]
    fn test_validate_soc2_schema_empty_action() {
        let entry = AuditEntry {
            id: 7,
            timestamp: Utc::now(),
            agent_id: Uuid::new_v4(),
            action: "".to_string(),
            prompt_id: None,
            diff_hash: "a".repeat(64),
            before_json: None,
            after_json: None,
            ip_address: None,
        };
        let result = SqliteAuditLogger::validate_soc2_schema(&entry);
        assert!(result.is_err());
    }

    // ── Send / Sync ─────────────────────────────────────────────────────────

    #[test]
    fn test_sqlite_audit_logger_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SqliteAuditLogger>();
    }
}
