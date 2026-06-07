# c1 — Wire `swarm::SwarmRoleRegistry` into PromptHub

**Epic:** swarm role management (first shippable slice: core wiring only).
**Scope:** prompt-hub crate only (hub.rs + lib.rs re-exports + 1 test). No CLI, no server route changes.

---

## 1. Files to modify

| File | Change |
|------|--------|
| `prompt-hub/src/hub.rs` | Add field, init in `new()`, add 3 methods, add tests |
| `prompt-hub/src/lib.rs` | Verify swarm re-export is already there (line 55) — **no edit needed** if `pub mod swarm;` exists |

---

## 2. Struct field addition

Add one line to `PromptHub` (around line 101, after the last field):

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
    swarm_registry: Arc<swarm::SwarmRoleRegistry>, // ← NEW
}
```

**Rationale:** `Arc` mirrors the pattern used by `storage`, `metrics`, and `sync`. Callers already expect `Arc` handles for shared-state managers. The registry is clone-on-read (already derived `Clone`), so wrapping in `Arc` adds zero overhead over a direct field.

---

## 3. Method signatures & bodies

### 3a. Init in `new()` — after line ~135 (after the last field)

In the existing `Self { ... }` literal, add the new field:

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
    swarm_registry: Arc::new(swarm::SwarmRoleRegistry::default_registry()), // ← NEW
};
```

**Why `Arc::new(...)`:** Keeps type consistent with other manager fields. The call is cheap — `SwarmRoleRegistry::default_registry()` is a fast HashMap insertion of ~5 standard roles. No async needed (registry creation has no I/O).

### 3b. `manage_swarm()` accessor (~line 160, after `metrics()`)

```rust
    /// Return a cloneable handle to the swarm role registry.
    pub fn manage_swarm(&self) -> Arc<swarm::SwarmRoleRegistry> {
        Arc::clone(&self.swarm_registry)
    }
```

This matches the pattern of `storage()`, `metrics()`, `search_engine()` accessors that the server layer uses.

### 3c. `validate_swarm_roles()` convenience method (~line 168, after accessor block)

```rust
    /// Validate a list of roles against the registry rules.
    ///
    /// Returns an empty Vec on success (no conflicts). Use this before
    /// attempting to register or reconfigure a swarm.
    pub fn validate_swarm_roles(&self, roles: &[crate::models::Role]) -> Result<Vec<swarm::Conflict>> {
        swarm::validate_swarm_roles(roles)
    }
```

### 3d. `generate_swarm_bundle()` convenience method (~line 175)

```rust
    /// Generate a swarm bundle using the current registry and role set.
    ///
    /// Validates roles, builds the dependency DAG, produces handoff templates,
    /// consistency report, and evolution suggestions in one call.
    pub async fn generate_swarm_bundle(
        &self,
        roles: Vec<crate::models::Role>,
        domain: crate::models::Domain,
        workflow_id: Uuid,
    ) -> Result<swarm::SwarmBundle> {
        swarm::generate_swarm_bundle(roles, domain, workflow_id).await
    }
```

---

## 4. Test strategy

Add a test module section at the end of `hub.rs` (inside the existing `#[cfg(test)] mod tests`).

**4a. `test_manage_swarm_returns_handle`** — verifies accessor returns a non-empty registry.
**4b. `test_validate_swarm_roles_ok`** — valid role set returns empty conflicts.
**4c. `test_validate_swarm_roles_missing_orchestrator`** — omits Orchestrator → gets Conflict.

```rust
    #[test]
    fn test_manage_swarm_returns_default_registry() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();
        let registry = hub.manage_swarm();
        // Default registry has the standard roles (Orchestrator, Architect, etc.)
        assert!(registry.list_roles().len() >= 5);
    }

    #[tokio::test]
    async fn test_validate_swarm_roles_ok() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();
        let roles = vec![
            models::Role::Orchestrator,
            models::Role::Architect,
            models::Role::Implementer,
        ];
        let conflicts = hub.validate_swarm_roles(&roles);
        assert!(conflicts.is_ok());
        assert!(conflicts.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_validate_swarm_roles_missing_orchestrator() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();
        let roles = vec![
            models::Role::Architect,
            models::Role::Implementer,
        ];
        let conflicts = hub.validate_swarm_roles(&roles);
        assert!(conflicts.is_ok());
        let c = conflicts.unwrap();
        assert!(!c.is_empty()); // should contain Conflict::MissingRole
    }
```

---

## 5. Verification commands

```bash
# 1. Default build check (no features)
just check

# 2. All-features build check
just check --all-targets   # or: just check

# 3. Clippy lint
just lint                  # must be clean (-D warnings)

# 4. Run the new tests specifically
cargo test -p prompt-hub -- swarm_registry manage_swarm validate_swarm

# 5. Full test suite (regression guard)
just test

# 6. Fmt check
just fmt
```

---

## 6. Acceptance criteria

- [ ] `just lint` passes with zero warnings/errors across `--all-targets`.
- [ ] `just check` passes (both default and `--all-features`).
- [ ] New tests pass individually and as a group.
- [ ] `PromptHub` still satisfies `Send + Sync` (existing `test_send_sync` in hub.rs must still compile — adding another `Arc<T>` field cannot break this since `SwarmRoleRegistry` derives `Clone`/`Debug` and all HashMap values are `Clone`).
- [ ] The `swarm::SwarmRoleRegistry` is constructed exactly once during `new()`.

---

## 7. Drift flagged

**None.** All proposed code is idiomatic Rust: native async, `Result/HubError`, `Arc` sharing, `tracing` annotations, serde-compatible derives. No foreign-language patterns detected.
