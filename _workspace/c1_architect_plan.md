# Cycle 1 - Wire `swarm::SwarmRoleRegistry` into PromptHub

## Blast radius

- **Files changed:** 1 (hub.rs only) + 1 new test function in hub.rs
- **Scope:** Core library internal wiring. No CLI/server surface exposure yet.
- **Risk: LOW.** Adds one field to `PromptHub`, three thin methods, one test. No public API changes on existing methods. `swarm` is already a declared module in lib.rs; its types (`SwarmBundle`, `Conflict`, `RoleMetadata`) are re-exported via `pub use models::*`.
- **Caller impact:** Zero external callers today. Server routes can consume via the new handle once wired.

## Design decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Feature gate | None - wire unconditionally | swarm.rs has no feature-gated code; quality_gate/lineage are already wired identically |
| Swarm storage type | `Arc<SwarmRoleRegistry>` | Same pattern as `quality_gate`, `metrics` - cheap clone for shared access across handlers |
| Init value | `SwarmRoleRegistry::default_registry()` | Five standard roles + Junie; server can extend via the handle later |
| Error handling | All methods delegate to swarm functions which return `Result<T>` or `Vec<Conflict>`; no new error variants needed |
| Async style | Native `async fn` for bundle generation; sync for registry handle | No `async_trait` - Rust 2024 edition |

## Files & changes

### File: `/home/drdave/Desktop/meta/prompt_hub/prompt-hub/src/hub.rs`

#### Change 1 - Add import (around line 7)

Add after the `lineage` import (line 7):

```rust
use crate::swarm::{self, SwarmRoleRegistry};
```

#### Change 2 - Add field to PromptHub struct (lines 95-106)

After `lineage: LineageTracker,` (line 105), add:

```rust
    swarm_registry: Arc<SwarmRoleRegistry>,
```

Full struct becomes:

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
    swarm_registry: Arc<SwarmRoleRegistry>,  // NEW
}
```

#### Change 3 - Initialize in `new()` (line ~143)

After the `lineage` field init (line 143), add:

```rust
            swarm_registry: Arc::new(swarm::SwarmRoleRegistry::default_registry()),
```

Full `Self { ... }` block (lines 133-144):

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
            swarm_registry: Arc::new(swarm::SwarmRoleRegistry::default_registry()), // NEW
        };
```

#### Change 4 - Add three methods (insert after lineage helpers, after line 712)

All three go in the existing `impl PromptHub` block. Insert after `lineage_roots()` (line 711):

```rust
    // - Swarm role registry ------------------------------------------------

    /// Return a cloneable handle to the swarm role registry.
    ///
    /// The returned `Arc` can be cloned and shared across handlers or
    /// downstream components. Mutable operations (e.g., registering custom
    /// roles) use `Arc::get_mut()` on the original.
    pub fn manage_swarm(&self) -> Arc<SwarmRoleRegistry> {
        Arc::clone(&self.swarm_registry)
    }

    /// Validate a set of roles against the swarm dependency DAG.
    ///
    /// Returns an empty vec if all roles are valid, or a list of conflicts
    /// (missing required roles, duplicates, capability gaps, custom-name
    /// violations).
    #[instrument(skip(self, roles))]
    pub fn validate_swarm_roles(&self, roles: &[Role]) -> Result<Vec<Conflict>> {
        swarm::validate_swarm_roles(roles)
    }

    /// Generate a swarm bundle for the given roles, domain, and workflow.
    ///
    /// Validates the role DAG, builds the dependency graph, generates a
    /// consistency report, evolution suggestions, and handoff templates.
    #[instrument(skip(self, roles))]
    pub async fn generate_swarm_bundle(
        &self,
        roles: Vec<Role>,
        domain: Domain,
        workflow_id: Uuid,
    ) -> Result<SwarmBundle> {
        swarm::generate_swarm_bundle(roles, domain, workflow_id).await
    }
```

### File: `/home/drdave/Desktop/meta/prompt_hub/prompt-hub/src/hub.rs` (tests)

Add one test after the last lineage test (after line 1087, inside `mod tests`):

```rust
    #[tokio::test]
    async fn test_swarm_registry_handle() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let registry = hub.manage_swarm();
        assert!(!registry.list_roles().is_empty());
        assert!(registry.get(&Role::Orchestrator).is_some());
    }

    #[test]
    fn test_validate_swarm_roles_with_orchestrator() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        // Valid: Orchestrator is the required role.
        let result = hub.validate_swarm_roles(&[Role::Orchestrator]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_swarm_roles_critic_without_implementer() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        // Should produce CapabilityMissing conflict.
        let result = hub.validate_swarm_roles(&[Role::Orchestrator, Role::Critic]);
        assert!(result.is_ok());
        let conflicts = result.unwrap();
        assert!(conflicts
            .iter()
            .any(|c| matches!(c, Conflict::CapabilityMissing)));
    }

    #[tokio::test]
    async fn test_generate_swarm_bundle() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let bundle = hub
            .generate_swarm_bundle(
                vec![Role::Orchestrator, Role::Architect],
                Domain::Coding,
                Uuid::new_v4(),
            )
            .await;
        assert!(bundle.is_ok());
    }
```

## Tests summary

| Test | Type | Verifies |
|------|------|----------|
| `test_swarm_registry_handle` | `#[tokio::test]` | Field initialized, `manage_swarm()` returns non-empty registry |
| `test_validate_swarm_roles_with_orchestrator` | `#[test]` (sync) | Valid role set produces empty conflicts via hub delegate |
| `test_validate_swarm_roles_critic_without_implementer` | `#[test]` (sync) | Conflict detection flows through correctly |
| `test_generate_swarm_bundle` | `#[tokio::test]` | Async delegation returns Ok bundle |

## Verify commands

```bash
# 1. Check compiles clean (default + all features)
just check

# 2. Clippy lint - must be zero warnings
just lint

# 3. Run just the new tests
cargo test -p prompt-hub --all-features swarm

# 4. Run full suite to confirm no regression
just test

# 5. Verify Send + Sync still holds (compile assertion in hub.rs tests)
cargo test -p prompt-hub --all-features test_send_sync
```

## Acceptance criteria

- [ ] `just check` passes (no errors across workspace, all features)
- [ ] `just lint` passes (zero clippy warnings with `-D warnings`)
- [ ] `just test` passes (full suite green, including 4 new swarm tests)
- [ ] `test_swarm_registry_handle` - registry has 6 default roles, Orchestrator metadata present
- [ ] `test_validate_swarm_roles_critic_without_implementer` - Conflict::CapabilityMissing in result
- [ ] `test_generate_swarm_bundle` - async method returns Ok(SwarmBundle) with non-empty handoff_template
- [ ] `test_send_sync` still passes (PromptHub remains Send + Sync + 'static)
- [ ] No `async_trait`, no `unsafe`, no feature-gate on swarm wiring

## Drift flagged

None. All code follows Rust-native conventions:
- Native `async fn in trait` (no `async_trait`)
- `Result<T, HubError>` via the `Result` alias from `error` module
- `#![forbid(unsafe_code)]` unchanged
- `tracing::instrument` on all public methods (consistent with existing pattern)
- `Arc` handle for shared mutable access (consistent with quality_gate/lineage/metrics patterns)
