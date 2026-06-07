# c2 — Wire `pollination::CrossAgentPollination` into PromptHub

**Backlog item:** Add pollination engine as a shared field on `PromptHub`, expose via thin accessor methods, and wire it so the server layer can query/rank patterns.

---

## 1. Blast Radius

| Symbol | Module(s) touched | Risk |
|--------|-------------------|------|
| `hub.rs::PromptHub` struct + impl | `prompt-hub/src/hub.rs` | **Medium** — public facade; existing tests in this file run on every cycle. |
| `pollination.rs` module (no external callers yet) | `prompt-hub/src/pollination.rs` | **Low** — 10 unit tests already pass; no other crate calls into it. |
| `lib.rs` (already has `pub mod pollination`) | `prompt-hub/src/lib.rs` line 41 | **None** — module declaration already exists. |

The new field is additive only (`Arc<Mutex<...>>`). No existing method signatures change. Server layer changes are optional for this cycle -- the accessor alone satisfies "wiring."

---

## 2. Design Decisions (Rust-native)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Mutex type | `std::sync::Mutex` | Pollination is fully sync; `tokio::Mutex` would add overhead for no async benefit. Matches `LockManager::locks` (hub.rs:41). |
| Wrapping | `Arc<Mutex<CrossAgentPollination>>` | Same pattern as `swarm_registry`, `quality_gate`, `storage`. Cheap `.clone()` gives shared handle. |
| Read accessor returns `Arc<...>` | Yes | Clone-on-call is cheap (Arc interior pointer). Matches `manage_swarm()` at hub.rs:723. |
| Write access | `pollination_mut(&mut self)` returning `&mut CrossAgentPollination` | Same pattern as `lineage_mut()` at hub.rs:697 -- avoid double-Arc allocation; inline field mutation via direct mutable borrow. |
| Error wrapping | All public methods return `Result<T, HubError>` | Consistent with the crate's `type Result<T> = std::result::Result<T, HubError>`. Engine methods that cannot fail (e.g., extract_patterns) are wrapped in `Ok(...)`. |
| Module import | `use crate::pollination::{CrossAgentPollination, Pattern};` inside hub.rs | Keep it explicit; pollination is not re-exported from lib root beyond the module path. |

---

## 3. Files & Changes

### File 1: `/home/drdave/Desktop/meta/prompt_hub/prompt-hub/src/hub.rs`

**Change A -- Import (after line 14, before line 15):**
```rust
use crate::pollination::{CrossAgentPollination, Pattern};
```

**Change B -- New field on `PromptHub` struct (after line 107, after `swarm_registry`):**
```rust
    pollination: Arc<std::sync::Mutex<CrossAgentPollination>>,
```

Full struct becomes (lines 96-108):
```rust
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
}
```

**Change C -- Initialize in `new()` (after line 146):**
```rust
            pollination: Arc::new(std::sync::Mutex::new(CrossAgentPollination::new())),
```

Full `Self { ... }` block becomes (lines 135-147):
```rust
        let mut hub = Self {
            storage,
            search_engine: hybrid,
            sanitizer: PromptSanitizer::default(),
            auth: RbacAuthManager::new(),
            lock_manager: LockManager::new(),
            metrics: metrics.clone(),
            sync: SyncManager::new(),
            hooks: HookRegistry::new(),
            quality_gate: Arc::new(QualityGate::new()),
            lineage: LineageTracker::new(),
            swarm_registry: Arc::new(swarm::SwarmRoleRegistry::default_registry()),
            pollination: Arc::new(std::sync::Mutex::new(CrossAgentPollination::new())),
        };
```

**Change D -- Accessor methods (after the swarm section, after line 749):**
```rust
    // - Cross-agent pollination ---------------------------------------------------

    /// Return a cloneable handle to the cross-agent pollination engine.
    ///
    /// The returned `Arc` can be cloned and shared across handlers. Mutable
    /// operations (e.g., sharing patterns) use `Arc::get_mut()` on the original.
    pub fn pollination(&self) -> Arc<std::sync::Mutex<CrossAgentPollination>> {
        Arc::clone(&self.pollination)
    }

    /// Extract reusable prompt patterns from a prompt.
    #[instrument(skip(self, prompt))]
    pub fn extract_pollination_patterns(
        &self,
        prompt: &Prompt,
    ) -> Result<Vec<Pattern>> {
        Ok(CrossAgentPollination::extract_patterns(prompt))
    }

    /// Rank all patterns in the pool by composite score.
    #[instrument(skip(self))]
    pub fn rank_pollination_patterns(
        &self,
        num_domains: usize,
    ) -> Result<Vec<(String, f64)>> {
        let engine = self.pollination.lock().map_err(|e| {
            HubError::Internal(format!("pollination mutex poisoned: {e}"))
        })?;
        Ok(engine.rank_patterns(num_domains))
    }

    /// Mutable access to the pollination engine (caller owns mutation).
    ///
    /// Prefer using this over cloning the Arc + holding a separate guard -- it
    /// avoids double-allocation and keeps the engine inline with PromptHub.
    pub fn pollination_mut(&mut self) -> &mut CrossAgentPollination {
        &mut *self.pollination.get_mut().expect("pollination mutex poisoned")
    }
```

### File 2: `/home/drdave/Desktop/meta/prompt_hub/prompt-hub/src/hub.rs` (tests)

**Change E -- Add pollination tests to `mod tests` in hub.rs (after line 1186, before the closing `}` of `mod tests`):**

```rust
    // - Pollination tests --------------------------------------------------------

    #[test]
    fn test_extract_pollination_patterns_step_by_step() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let prompt = Prompt {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: "Follow these steps: 1. Plan 2. Execute".to_string(),
            user_template: "Help me.".to_string(),
            required_vars: vec![],
            domain: Domain::Coding,
            tags: vec![],
            target_roles: vec![],
            metadata: Default::default(),
            metrics: PromptMetrics {
                usage_count: 50,
                success_rate: 0.9,
                avg_tokens: 300,
                avg_latency_ms: 100,
                last_used: Some(chrono::Utc::now()),
                cost_estimate_usd: 0.0,
            },
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            author: AgentIdentity {
                id: Uuid::new_v4(),
                name: "test".to_string(),
                capabilities: Default::default(),
                token_hash: "".to_string(),
                specialization_score: 0.5,
            },
            deleted_at: None,
            generation_params: None,
            locale: None,
            multimodal: None,
        };

        let patterns = hub.extract_pollination_patterns(&prompt).unwrap();
        assert!(
            patterns.iter().any(|p| p.structure == "step-by-step"),
            "Should detect step-by-step pattern"
        );
    }

    #[test]
    fn test_pollination_handle_returns_arc() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let handle1 = hub.pollination();
        let handle2 = hub.pollination();
        // Same Arc interior -- pool should agree
        assert_eq!(handle1.lock().unwrap().pool_size(), 0);
        assert_eq!(Arc::strong_count(&handle1), Arc::strong_count(&handle2));
    }

    #[test]
    fn test_pollination_mut_share_pattern() {
        let dir = TempDir::new().unwrap();
        let mut hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let pattern = pollination::Pattern {
            id: Uuid::new_v4(),
            structure: "few-shot".to_string(),
            domains: vec![Domain::Writing],
            score: 0.8,
            usage_count: 10,
            agent_id: Uuid::new_v4(),
            example_snippet: "Here is an example...".to_string(),
        };

        hub.pollination_mut()
            .share_pattern(pattern)
            .unwrap();
        assert_eq!(hub.pollination().lock().unwrap().pool_size(), 1);
    }
```

---

## 4. Migrations

None. The pollination engine uses an in-process `HashMap` -- no persistence layer required for this cycle.

---

## 5. Tests

| # | Test file | What it verifies | Type |
|---|-----------|------------------|------|
| T1 | `hub.rs::test_extract_pollination_patterns_step_by_step` | `extract_pollination_patterns()` detects the "step-by-step" structure keyword from a Prompt | unit (sync) |
| T2 | `hub.rs::test_pollination_handle_returns_arc` | `pollination()` returns an Arc whose interior is shared (strong_count check); pool_size agrees after creation | unit (sync) |
| T3 | `hub.rs::test_pollination_mut_share_pattern` | `pollination_mut()` allows mutation; a shared Pattern appears in the pool read via `pollination().lock()` | unit (sync) |

Existing pollination tests (10 in `pollination.rs`) are unaffected and already cover extract, share, score, rank, cross_pollinate, clear, find_by_structure paths.

---

## 6. Verify Commands

```bash
# 1. Check default features compile (includes all features since pollition has no feature gate)
just check

# 2. Clippy -- must be clean with -D warnings
just lint

# 3. Run the new hub tests specifically
cargo test -p prompt-hub --lib hub::tests::test_extract_pollination_patterns_step_by_step
cargo test -p prompt-hub --lib hub::tests::test_pollination_handle_returns_arc
cargo test -p prompt-hub --lib hub::tests::test_pollination_mut_share_pattern

# 4. Run full test suite (ensure nothing regressed)
just test

# 5. Verify Send + Sync still holds (existing test at hub.rs:926 -- confirm it still passes)
cargo test -p prompt-hub --lib hub::tests::test_send_sync
```

---

## 7. Acceptance Criteria

- [ ] `just check` passes with no warnings across all features.
- [ ] `just lint` passes (clippy `-D warnings` clean).
- [ ] `cargo test -p prompt-hub --lib hub::tests::test_extract_pollination_patterns_step_by_step` passes.
- [ ] `cargo test -p prompt-hub --lib hub::tests::test_pollination_handle_returns_arc` passes.
- [ ] `cargo test -p prompt-hub --lib hub::tests::test_pollination_mut_share_pattern` passes.
- [ ] `just test` passes (full suite including the 10 existing pollination tests and all other hub tests).
- [ ] `cargo test -p prompt-hub --lib hub::tests::test_send_sync` still passes -- `Arc<Mutex<T>>` field must not break Send+Sync.

---

## 8. Drift Flagged

None. This plan is fully Rust-native:
- `std::sync::Mutex`, not tokio or other runtime-specific mutexes.
- All methods return `Result<T, HubError>` per crate convention.
- Uses `Arc<Mutex<...>>` following the existing `swarm_registry` pattern exactly.
- Inline mutable accessor (`pollination_mut`) mirrors `lineage_mut()` -- no double-Arc.
- No feature gates needed -- pollination has no feature flag in Cargo.toml, so it compiles unconditionally (already declared in lib.rs:41).
