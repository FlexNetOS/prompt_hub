# Cycle 4 — Wire `satisfaction::SatisfactionTracker` into PromptHub

## 1. Blast Radius

**Symbols touched:**
- `prompt-hub/src/hub.rs:97` — `PromptHub` struct definition (add field)
- `prompt-hub/src/hub.rs:136-150` — `PromptHub::new()` (initialize field)
- `prompt-hub/src/hub.rs:~892` — tests module (add satisfaction tests)

**Risk assessment:** **Low** — single struct + one constructor. No caller of existing code changes. The `SatisfactionTracker` owns its own internal `Arc<RwLock<T>>` (already thread-safe), so no external locking needed. The field itself is an `Arc<SatisfactionTracker>` which is naturally `Clone`+`Send`+`Sync`.

**Module visibility:** `pub mod satisfaction;` already declared at `lib.rs:50`. All public types (`SatisfactionTracker`, `RatingEntry`, `RatingKind`, `SuccessEvent`, `SatisfactionMetrics`, `TrendDirection`) are already `pub`.

## 2. Rust-Native Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Field type on PromptHub | `satisfaction_tracker: Arc<SatisfactionTracker>` | Internal tracker has its own `Arc<RwLock<T>>` for ratings/events. External share needs `Arc<SatisfactionTracker>`. Matches the pattern used for `metrics`, `quality_gate`, `swarm_registry`. |
| Constructor init | `Arc::new(SatisfactionTracker::new(1000))` | Default capacity 1000 per the existing `Default` impl on `SatisfactionTracker`. Consistent, zero-config. |
| Public methods on PromptHub | `satisfaction_tracker()` (handle) + 4 thin delegates (`record_csat_rating`, `record_nps_rating`, `record_satisfaction_event`, `satisfaction_metrics`) | Server layer needs to record and query satisfaction without importing the tracker type directly. Thin passthrough follows the existing pattern (e.g., `pollination()`, `manage_swarm()`). |
| Error path for metrics | `Result<SatisfactionMetrics, HubError>` | The tracker's internal `metrics()` is infallible (reads in-memory VecDeques with Arc<RwLock>). Wrapping in `Ok(...)` via `Result` lets callers uniformly handle it. No actual failure paths exist yet but the type signature stays conservative. |
| Feature gate | None | `satisfaction` module is un-gated (`lib.rs:50`). Types are self-contained — no foreign dependencies. |
| Sync vs Async methods | All sync (not `async fn`) | Tracker operations read/write in-memory VecDeques under a RwLock — fast, no I/O, no need to be async. Matches how other non-IO fields work on PromptHub. |

## 3. Files & Changes

### File 1: `prompt-hub/src/hub.rs`

**Change A — Import (add to `use crate::` block after line ~14):**
```rust
use crate::satisfaction::{SatisfactionMetrics, SatisfactionTracker};
```

**Change B — Struct field (inside `PromptHub` at line ~109):**
Add one field to the struct:
```rust
    satisfaction_tracker: Arc<SatisfactionTracker>,
```

Final struct reads:
```rust
#[derive(Debug)]
pub struct PromptHub {
    storage: Arc<Storage>,
    search_engine: Arc<HybridEngine>,
    sanitizer: PromptSanitizer,
    auth: RbacAuthManager,
    lock_manager: LockManager,
    metrics: Arc<MetricsCollector>,
    sync: SyncManager,
    hooks: HookRegistry,
    quality_gate: Arc<QualityGate>,
    lineage: LineageTracker,
    swarm_registry: Arc<SwarmRoleRegistry>,
    pollination: Arc<std::sync::Mutex<CrossAgentPollination>>,
    satisfaction_tracker: Arc<SatisfactionTracker>,  // <-- NEW
}
```

**Change C — Constructor (inside `new()`, after last existing field initializer at ~line 149):**
```rust
            satisfaction_tracker: Arc::new(SatisfactionTracker::new(1000)),
```

**Change D — New public methods (insert after the pollination section, before closing `impl PromptHub` brace at line ~792):**
```rust
    // - User satisfaction tracker --------------------------------------------------

    /// Return a cloneable handle to the user satisfaction tracker.
    ///
    /// The returned `Arc` can be cloned and shared across handlers. Mutable
    /// operations (e.g., recording ratings) use the provided delegate methods
    /// or call into the tracker directly via this handle.
    pub fn satisfaction_tracker(&self) -> Arc<SatisfactionTracker> {
        Arc::clone(&self.satisfaction_tracker)
    }

    /// Record a CSAT rating (1-5), delegated to the satisfaction tracker.
    #[instrument(skip(self))]
    pub fn record_csat_rating(&self, score: u8, context: &str) {
        self.satisfaction_tracker.record_csat(score, context);
    }

    /// Record an NPS rating (1-10), delegated to the satisfaction tracker.
    #[instrument(skip(self))]
    pub fn record_nps_rating(&self, score: u8) {
        self.satisfaction_tracker.record_nps(score);
    }

    /// Record a success/failure event in the satisfaction funnel.
    #[instrument(skip(self))]
    pub fn record_satisfaction_event(&self, prompt_id: &str, successful: bool, attempts: u8) {
        self.satisfaction_tracker.record_event(prompt_id, successful, attempts);
    }

    /// Query current satisfaction metrics.
    #[instrument(skip(self))]
    pub fn satisfaction_metrics(&self) -> Result<SatisfactionMetrics> {
        Ok(self.satisfaction_tracker.metrics())
    }
```

### File 2: `prompt-hub/src/lib.rs` — NO CHANGES
`pub mod satisfaction;` already present at line 50. All public types are exposed.

### File 3: `prompt-hub/src/models.rs` — NO CHANGES
All satisfaction types (`SatisfactionMetrics`, etc.) are self-contained in `satisfaction.rs`. No model additions needed.

## 4. Migrations

None. SatisfactionTracker is pure in-memory (VecDeque-backed). No schema changes.

## 5. Tests

Add to `prompt-hub/src/hub.rs` tests module (after the last existing test at ~line 1314):

```rust
    // - Satisfaction tracker tests -------------------------------------------

    #[tokio::test]
    async fn test_satisfaction_tracker_handle() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let handle1 = hub.satisfaction_tracker();
        let handle2 = hub.satisfaction_tracker();
        assert_eq!(Arc::strong_count(&handle1), Arc::strong_count(&handle2));
        // Default tracker has zero ratings/events.
        assert_eq!(handle1.rating_count(), 0);
        assert_eq!(handle1.event_count(), 0);
    }

    #[tokio::test]
    async fn test_record_csat_via_hub() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.record_csat_rating(5, "Great UX");
        hub.record_csat_rating(3, "Okay experience");

        let tracker = hub.satisfaction_tracker();
        assert_eq!(tracker.rating_count(), 2);
        let metrics = tracker.metrics();
        assert_eq!(metrics.csat_average, 4.0);
    }

    #[tokio::test]
    async fn test_record_nps_via_hub() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.record_nps_rating(10); // promoter
        hub.record_nps_rating(9);  // promoter
        hub.record_nps_rating(4);  // detractor

        let metrics = hub.satisfaction_metrics().unwrap();
        // (2 - 1) / 3 * 100 = 33.33...
        assert!(
            (metrics.nps_score - 33.33).abs() < 0.1,
            "NPS score: {}",
            metrics.nps_score
        );
    }

    #[tokio::test]
    async fn test_record_event_via_hub() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.record_satisfaction_event("p1", true, 1);
        hub.record_satisfaction_event("p2", true, 3);
        hub.record_satisfaction_event("p3", false, 1);

        let tracker = hub.satisfaction_tracker();
        assert_eq!(tracker.event_count(), 3);
        assert_eq!(tracker.one_shot_success_rate(), 50.0);
    }

    #[tokio::test]
    async fn test_satisfaction_metrics_empty() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let metrics = hub.satisfaction_metrics().unwrap();
        assert_eq!(metrics.csat_average, 0.0);
        assert_eq!(metrics.nps_score, 0.0);
        assert_eq!(metrics.one_shot_success_rate, 0.0);
        assert_eq!(metrics.total_ratings, 0);
        assert_eq!(metrics.total_events, 0);
        assert_eq!(metrics.recent_trend, crate::satisfaction::TrendDirection::Stable);
    }

    #[tokio::test]
    async fn test_csat_invalid_silent() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.record_csat_rating(0, "invalid");  // should be silently ignored
        hub.record_csat_rating(6, "invalid");  // should be silently ignored
        hub.record_csat_rating(3, "valid");     // should count

        let tracker = hub.satisfaction_tracker();
        assert_eq!(tracker.rating_count(), 1);
    }
```

## 6. Verify Commands

```bash
# Check compilation (all features)
just check

# Run clippy (must be clean -D warnings)
just lint

# Run the new tests
cargo test -p prompt-hub --test-threads=1 test_satisfaction

# Run full test suite to verify no regression
just test

# Verify formatting is clean
just fmt
```

## 7. Acceptance Criteria

| # | Criterion | Check |
|---|-----------|-------|
| 1 | `PromptHub` struct compiles with `satisfaction_tracker: Arc<SatisfactionTracker>` field | `just check` passes |
| 2 | Constructor initializes tracker at capacity 1000 via `Arc::new(SatisfactionTracker::new(1000))` | New tests in `test_satisfaction_tracker_handle` confirm zero ratings at startup |
| 3 | `satisfaction_tracker()` returns an `Arc` that can be cloned and shared across handlers | Test `test_satisfaction_tracker_handle` — asserts `Arc` strong count matches |
| 4 | `record_csat_rating(5, "ctx")` records a CSAT entry visible via the tracker handle | Test `test_record_csat_via_hub` — asserts rating_count() == 2 and csat_average == 4.0 |
| 5 | `record_nps_rating()` records entries and `satisfaction_metrics().unwrap().nps_score` is correct | Test `test_record_nps_via_hub` — asserts NPS score within tolerance |
| 6 | `record_satisfaction_event()` counts events correctly and one-shot rate matches | Test `test_record_event_via_hub` — event_count == 3, one_shot == 50.0% |
| 7 | Empty tracker returns zeroed metrics with `Stable` trend via `satisfaction_metrics()` | Test `test_satisfaction_metrics_empty` |
| 8 | Invalid CSAT scores (0, 6) are silently ignored, valid scores counted | Test `test_csat_invalid_silent` — rating_count == 1 |
| 9 | `just lint` (clippy -D warnings) is clean across the workspace | `just lint` passes |
| 10 | `#[forbid(unsafe_code)]` still holds | No `unsafe` introduced in any modified file |

## 8. Drift Flagged

None. The satisfaction module is already Rust-native: no `async_trait`, no `unsafe`, uses native methods with internal `Arc<RwLock<T>>`, no non-Cargo dependencies, no foreign-language snippets. The existing `record_csat`/`record_nps`/`record_event` methods are sync (which we respect by keeping hub delegates sync too).

## 9. Post-Merge Note (for next cycle)

The satisfaction tracker is now accessible from the server layer via `Arc<PromptHub>::satisfaction_tracker()`. Next natural steps:
- Wire it into the CLI as a `prompthub metrics` subcommand or `prompthub csat <score>` command.
- Add an HTTP endpoint (`/api/satisfaction`) in `prompthub-server`.
- Persist satisfaction data to SQLite (add column to prompts table or new `satisfaction` table + migration).
- Connect `record_satisfaction_event` to the actual execution pipeline (auto-record on prompt success/failure).
