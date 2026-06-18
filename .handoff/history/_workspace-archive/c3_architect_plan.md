# Cycle 3 — Wire `lineage::LineageTracker` into PromptHub

## 1. Blast Radius

**Symbols touched:**
- `prompt-hub/src/hub.rs:94` — `PromptHub` struct definition (add field)
- `prompt-hub/src/hub.rs:110-147` — `PromptHub::new()` (initialize field)
- `prompt-hub/src/lib.rs:32` — `pub mod lineage;` already exists (no change needed)

**Risk assessment:** **Low** — single struct + one constructor. No caller of existing code changes. The `LineageTracker` is `Default`, `Clone`, `Debug` — zero-unsafe, no trait boundaries.

## 2. Rust-Native Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Storage for tracker | `LineageTracker` (owned, not `Arc`) | Tracker is per-hub-mutable state; `Clone`+`Default` on lineage.rs means cheap copies. Unlike `QualityGate` (shared via `Arc` from server), lineage is written by the hub thread only. |
| Field type | `lineage::LineageTracker` | Direct import — no re-export needed since it's not a public API surface for this cycle. |
| Public methods on PromptHub | `register_lineage_version` + `get_lineage_ancestry` + `detect_forks` + `get_descendants` + `build_lineage_tree` | Thin passthrough over tracker's in-memory graph. No DB integration yet (that's a future cycle). |
| Error path | `Result<_, HubError>` | Reuses existing error type — `HubError::NotFound`, `HubError::Conflict` already mapped in lineage.rs internal calls. |
| Feature gate | None | `lineage` is un-gated; module declared at lib.rs:32 with no `#[cfg(feature)]`. All dependent types (`LineageNode`, `Fork`, `AncestryPath`, `LineageTree`) are already in tree. |

## 3. Files & Changes

### File 1: `prompt-hub/src/hub.rs`

**Change A — Import (after line ~8):**
Add to the existing `use crate::` block (insert after line 9):
```rust
use crate::lineage::{LineageNode, LineageTracker};
```

**Change B — Struct field (inside `PromptHub` at line ~103):**
Add one field to the struct:
```rust
    lineage: LineageTracker,
```

Final struct should read:
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
    lineage: LineageTracker,  // <-- NEW
}
```

**Change C — Constructor (inside `new()` at ~line 140):**
Add to the struct initializer after the last existing field:
```rust
            lineage: LineageTracker::new(),
```

**Change D — New public methods (insert after `run_quality_gate`, at end of impl block ~line 659, before closing brace):**
```rust
    // ── Version lineage ────────────────────────────────────────────────

    /// Register a new prompt version in the lineage graph.
    #[instrument(skip(self))]
    pub async fn register_lineage_version(
        &self,
        version_id: &str,
        prompt_id: &str,
        parent_id: Option<&str>,
        author: &str,
    ) -> Result<()> {
        // LineageTracker is not Sync on &self (it's mutable), so we
        // cannot call &mut self from this async method. We store a
        // separate Arc<Mutex<>> in a future cycle; for now the caller
        // retrieves via lineage_mut() and mutates directly.
        let tracker = self.lineage_mut();
        tracker.register_version(version_id, prompt_id, parent_id, author)
    }

    /// Return a mutable reference to the lineage tracker.
    #[allow(clippy::mutable_key_type)]
    pub fn lineage_mut(&mut self) -> &mut LineageTracker {
        &mut self.lineage
    }

    /// Get the ancestry path for a version, delegated to the tracker.
    #[instrument(skip(self))]
    pub fn get_lineage_ancestry(&self, version_id: &str) -> Result<AncestryPath> {
        self.lineage.get_ancestry(version_id)
    }

    /// Detect all forks in the lineage graph.
    #[instrument(skip(self))]
    pub fn detect_lineage_forks(&self) -> Vec<Fork> {
        self.lineage.detect_forks()
    }

    /// Get all descendant version IDs for a root version.
    #[instrument(skip(self))]
    pub fn get_lineage_descendants(&self, version_id: &str) -> Vec<String> {
        self.lineage.get_descendants(version_id)
    }

    /// Build a lineage tree rooted at the given version ID.
    #[instrument(skip(self))]
    pub fn build_lineage_tree(&self, root_version: &str) -> Option<LineageTree> {
        self.lineage.build_tree(root_version)
    }
```

**Wait — critical design correction.** The above has a borrowing conflict: `register_lineage_version` takes `&self` but needs `&mut self` to call `lineage_mut()`. Since the existing hub methods all take `&self`, we have two options:

**Option A (correct for this cycle):** Make the field `Arc<Mutex<LineageTracker>>` like `lock_manager` could be — but that's heavier than needed.

**Option B (simpler, Rust-native for this scaffold):** Only expose query methods (which take `&self`) and a separate `lineage_mut()` which takes `&mut self`. The caller in server layer would need `Arc<PromptHub>` → use `lineage_mut()` from a single-threaded context.

Given the prompt says "keep changes minimal — one shippable commit" and this is the *last cycle of this session*, I recommend:

**Final design for C3:** Only expose **read-only** lineage query methods (all take `&self`), plus leave the tracker as plain `LineageTracker`. Write access can happen via mutable hub reference when needed. This matches how many other fields are handled (they have direct field access in the same module).

### File 1 revised — Change D (final):

```rust
    // ── Version lineage ────────────────────────────────────────────────

    /// Get the ancestry path for a version.
    #[instrument(skip(self))]
    pub fn get_lineage_ancestry(&self, version_id: &str) -> Result<AncestryPath> {
        self.lineage.get_ancestry(version_id)
    }

    /// Detect all forks in the lineage graph.
    #[instrument(skip(self))]
    pub fn detect_lineage_forks(&self) -> Vec<Fork> {
        self.lineage.detect_forks()
    }

    /// Get all descendant version IDs for a root version.
    #[instrument(skip(self))]
    pub fn get_lineage_descendants(&self, version_id: &str) -> Vec<String> {
        self.lineage.get_descendants(version_id)
    }

    /// Build a lineage tree rooted at the given version ID.
    #[instrument(skip(self))]
    pub fn build_lineage_tree(&self, root_version: &str) -> Option<LineageTree> {
        self.lineage.build_tree(root_version)
    }

    /// Mutable access to the lineage tracker (caller owns mutation).
    ///
    /// Prefer using this over storing a separate Arc/Mutex — it avoids
    /// double-allocation and keeps the tracker inline with PromptHub.
    #[allow(clippy::mutable_key_type)]
    pub fn lineage_mut(&mut self) -> &mut LineageTracker {
        &mut self.lineage
    }

    /// Number of registered version nodes.
    pub fn lineage_node_count(&self) -> usize {
        self.lineage.node_count()
    }

    /// Check whether a specific version is tracked.
    pub fn has_lineage_version(&self, version_id: &str) -> bool {
        self.lineage.has_version(version_id)
    }

    /// Get the set of root versions (no parents).
    pub fn lineage_roots(&self) -> &[String] {
        self.lineage.roots()
    }
```

### File 2: `prompt-hub/src/lib.rs` — NO CHANGES
The `pub mod lineage;` is already present at line 32. All public types (`LineageNode`, `LineageTracker`, `Fork`, `AncestryPath`, `LineageTree`) are already pub.

### File 3: `prompt-hub/src/models.rs` — NO CHANGES
All lineage types are self-contained in lineage.rs. No model additions needed.

## 4. Migrations

None. LineageTracker is pure in-memory (HashMap-backed). No schema changes.

## 5. Tests

Add to `prompt-hub/src/hub.rs` tests module (after existing test at ~line 898):

```rust
    #[tokio::test]
    async fn test_lineage_register_and_ancestry() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        // Register a root version.
        hub.lineage_mut()
            .register_version("v1", "prompt-a", None, "alice")
            .unwrap();
        assert_eq!(hub.lineage_node_count(), 1);
        assert_eq!(hub.lineage_roots().len(), 1);

        // Register a child version.
        hub.lineage_mut()
            .register_version("v2", "prompt-a", Some("v1"), "bob")
            .unwrap();

        let ancestry = hub.get_lineage_ancestry("v2").unwrap();
        assert_eq!(ancestry.path, vec!["v1", "v2"]);
        assert_eq!(ancestry.depth, 2);
    }

    #[tokio::test]
    async fn test_lineage_fork_detection() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.lineage_mut()
            .register_version("v1", "prompt-a", None, "alice")
            .unwrap();
        hub.lineage_mut()
            .register_version("v2", "prompt-a", Some("v1"), "bob")
            .unwrap();
        hub.lineage_mut()
            .register_version("v3", "prompt-a", Some("v1"), "charlie")
            .unwrap();

        let forks = hub.detect_lineage_forks();
        assert_eq!(forks.len(), 1);
        assert_eq!(forks[0].fork_point_version, "v1");
        assert_eq!(forks[0].branches.len(), 2);
    }

    #[tokio::test]
    async fn test_lineage_tree_build() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.lineage_mut()
            .register_version("v1", "prompt-a", None, "alice")
            .unwrap();
        hub.lineage_mut()
            .register_version("v2", "prompt-a", Some("v1"), "bob")
            .unwrap();

        let tree = hub.build_lineage_tree("v1").unwrap();
        assert_eq!(tree.root, "v1");
        assert_eq!(tree.nodes.len(), 2);
        assert!(tree.fork_count == 0); // only one child of v1
    }

    #[tokio::test]
    async fn test_lineage_descendants() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.lineage_mut()
            .register_version("v1", "prompt-a", None, "alice")
            .unwrap();
        hub.lineage_mut()
            .register_version("v2", "prompt-a", Some("v1"), "bob")
            .unwrap();
        hub.lineage_mut()
            .register_version("v3", "prompt-a", Some("v2"), "charlie")
            .unwrap();

        let descs = hub.get_lineage_descendants("v1");
        assert_eq!(descs.len(), 2);
        assert!(descs.contains(&"v2".to_string()));
        assert!(descs.contains(&"v3".to_string()));
    }

    #[tokio::test]
    async fn test_lineage_has_version() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        assert!(!hub.has_lineage_version("v99"));

        hub.lineage_mut()
            .register_version("v1", "prompt-a", None, "alice")
            .unwrap();

        assert!(hub.has_lineage_version("v1"));
        assert!(!hub.has_lineage_version("v99"));
    }

    #[tokio::test]
    async fn test_lineage_duplicate_conflict() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        hub.lineage_mut()
            .register_version("v1", "prompt-a", None, "alice")
            .unwrap();

        let result = hub.lineage_mut().register_version(
            "v1", // same ID
            "prompt-b",
            None,
            "bob",
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lineage_missing_parent() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let result = hub.lineage_mut().register_version(
            "v2",
            "prompt-a",
            Some("nonexistent"),
            "bob",
        );
        assert!(result.is_err());
    }
```

## 6. Verify Commands

```bash
# Check compilation (all features)
just check

# Run clippy (must be clean -D warnings)
just lint

# Run the new tests
cargo test -p prompt-hub --test-threads=1 test_lineage

# Run full test suite to verify no regression
just test
```

## 7. Acceptance Criteria

| # | Criterion | Check |
|---|-----------|-------|
| 1 | `PromptHub` struct compiles with `lineage: LineageTracker` field | `just check` passes |
| 2 | All new methods are callable from server layer via `Arc<PromptHub>` or `&mut PromptHub` | Method signatures compile in any consumer |
| 3 | `has_lineage_version("v1")` returns false before register, true after | Test `test_lineage_has_version` |
| 4 | `get_lineage_ancestry("v2")` returns path `["v1", "v2"]` when v1→v2 chain registered | Test `test_lineage_register_and_ancestry` |
| 5 | `detect_lineage_forks()` returns 1 fork when two children share a parent | Test `test_lineage_fork_detection` |
| 6 | `build_lineage_tree("v1")` returns tree with correct root, node count, depth | Test `test_lineage_tree_build` |
| 7 | `get_lineage_descendants("v1")` returns all descendant IDs transitively | Test `test_lineage_descendants` |
| 8 | Duplicate version registration returns `Err(HubError::Conflict(...))` | Test `test_lineage_duplicate_conflict` |
| 9 | Register child of non-existent parent returns `Err(HubError::NotFound(...))` | Test `test_lineage_missing_parent` |
| 10 | `just lint` (clippy -D warnings) is clean across the workspace | `just lint` passes |
| 11 | `#[forbid(unsafe_code)]` still holds | No `unsafe` introduced in any modified file |

## 8. Drift Flagged

None. The lineage module is already Rust-native: no `async_trait`, no `unsafe`, uses `HubError`/`Result`, native `async fn` in trait for its SearchEngine-adjacent methods (though LineageTracker itself is sync-only). Types are self-contained — no foreign-language snippets or non-Cargo dependencies.

## 9. Post-Merge Note (for next cycle)

The `&mut self` requirement for `lineage_mut()` means the server layer (which holds `Arc<PromptHub>`) cannot write to lineage without upgrading. The natural next step is:
- Wrap as `parking_lot::Mutex<LineageTracker>` or use a dedicated storage-backed `LineageStore`, or
- Use `std::sync::Mutex` (already available in the tree) — but this adds a compile dep.

This is deferred; C3 only delivers read queries + mutable access for callers who hold `&mut`.
