# s11c3 — Wire `circuit_breaker` into PromptHub Facade

**Item**: circuit_breaker (7.9K lines, 9 tests, `feature="circuit-breaker"`)
**Blast radius**: Low — module has zero callers in hub.rs; wiring adds new pub API only.
**Risk classification**: **Low** — no existing call sites to update; all changes are additive within PromptHub struct + hub.rs delegation methods.

---

## 1. Blast Radius

### Callers of `circuit_breaker` types in current codebase
- `hub.rs`: **0 callers** (confirmed via grep)
- `prompthub-server/src/routes.rs`: **0 callers**
- `prompthub/` CLI: **0 callers**

The `circuit_breaker` module is feature-gated and completely unwired. Adding it to the facade introduces new fields/methods only — no existing code breaks.

### Module self-tests (pre-existing, 9 tests in `circuit_breaker.rs`)
All run under `#[cfg(feature = "circuit-breaker")]` within the module itself. No changes needed there.

---

## 2. Design Decisions (Rust-native)

| Decision | Rationale |
|----------|-----------|
| **Feature gate**: `#[cfg(feature = "circuit-breaker")]` | Already declared in lib.rs:30 and Cargo.toml:56. Matches pattern from `budget` wiring (s11c2). |
| **Storage**: `Arc<CircuitBreaker>` (not raw) | Consistent with how health_monitor, load_balancer, budget_tracker are stored — always `Arc<...>` for shared access from server/CLI layers. |
| **Default values**: Use `CircuitBreaker::default()` (5 failures / 30s timeout) | No config field needed in `HubConfig` — the module's `Default` impl provides sensible defaults, same pattern as `BudgetTracker::default()`. |
| **Accessor method name**: `circuit_breaker()` returning `Arc<CircuitBreaker>` | Follows the naming convention: `health_monitor()`, `load_balancer()`, `metrics()`. Returns Arc for cheap cloning. |
| **Delegation methods**: None initially. Expose via accessor only. | The circuit breaker is a low-level utility consumed by other future modules (e.g., provider_health, load_balancer). Wiring it as a hub field makes it discoverable; callers access it through the accessor. No need to create wrapper methods until there's a concrete consumer that needs them. If one is found later, add delegation then. |
| **`#[cfg(feature = "circuit-breaker")]` on all additions** | Feature-gate every change line-by-line (import, field, init, accessor) so `--no-default-features` stays clean. |

---

## 3. Files & Changes

### File 1: `prompt-hub/src/hub.rs`

#### Change A: Feature-gated import (line 14 area — with other use statements)

**Line**: ~15 (after the existing provider_health import on line 14)

Add after line 15:
```rust
#[cfg(feature = "circuit-breaker")]
use crate::circuit_breaker::CircuitBreaker;
```

#### Change B: New struct field (inside `PromptHub` struct, lines 121-139)

**Line**: 138 (after `budget_tracker`, before the closing `}`)

Add after line 138:
```rust
    #[cfg(feature = "circuit-breaker")]
    circuit_breaker: Arc<CircuitBreaker>,
```

#### Change C: Constructor init (inside `Self { ... }` block, lines 180-200)

**Line**: 199 (after `budget_tracker`, before the closing `};`)

Add after line 199:
```rust
            #[cfg(feature = "circuit-breaker")]
            circuit_breaker: Arc::new(CircuitBreaker::default()),
```

#### Change D: Info log in constructor (after line 206)

**Line**: 206 (after the `#[cfg(feature = "budget")]` info block, before `Ok(hub)`)

Add after line 206:
```rust
        #[cfg(feature = "circuit-breaker")]
        info!("Circuit breaker initialized with defaults (threshold=5, timeout=30s)");
```

#### Change E: Accessor method (inside the `impl PromptHub` block, ~line 1439 area — after budget methods, before the Send+Sync block)

**Line**: After line 1460 (`}` closing impl) but before line 1462 (`// ---------------------------------------------------------------------------`)

Add:
```rust
    // ── Circuit breaker ----------------------------------------------------------

    /// Return a cloneable handle to the circuit breaker.
    #[cfg(feature = "circuit-breaker")]
    pub fn circuit_breaker(&self) -> Arc<CircuitBreaker> {
        Arc::clone(&self.circuit_breaker)
    }
```

#### Change F: Integration test (inside `mod tests`, ~line 1482 area)

Add a new test at the end of the existing `mod tests` block (after the last existing test, before its closing `}`):

```rust
    #[cfg(feature = "circuit-breaker")]
    #[tokio::test]
    async fn test_circuit_breaker_accessor() {
        let hub = PromptHub::new(
            temp_dir.path(),
            HubConfig::default(),
        )
        .await
        .unwrap();

        let cb = hub.circuit_breaker();
        assert_eq!(cb.current_state(), "closed");

        // Verify it can gate a failure
        let result = cb.call(|| Err::<(), _>(crate::error::HubError::Internal("test".into())));
        assert!(result.is_err());

        // After 5 consecutive failures it should open
        for _ in 0..4 {
            let _ = cb.call(|| Err::<(), _>(crate::error::HubError::Internal("test".into())));
        }
        assert_eq!(cb.current_state(), "open");
    }
```

**Note**: This test requires `temp_dir` to be in scope — check how the existing tests set up their `TempDir`. If `temp_dir: TempDir` is a field or local, ensure this test reuses it. See Change G below for the full context.

---

## 4. Migrations

**None**. The circuit breaker is an in-memory state machine (`Arc<RwLock<BreakerState>>`). No schema changes needed.

---

## 5. Tests

### Pre-existing (no changes)
- `circuit_breaker.rs` lines 142-232: 9 unit tests already verify all state transitions under the feature gate.

### New integration test (Change F above)
- `test_circuit_breaker_accessor`: verifies the accessor works, returns a valid CB in closed state, and that failures accumulate correctly to open.

---

## 6. Verify Commands

Run these **in order** after applying changes:

```bash
# 1. Check default features (must pass — circuit-breaker is NOT a default feature)
just check

# 2. Check all features (includes circuit-breaker)
just check --all-features
# or equivalently: cargo check --workspace --all-features

# 3. Clippy — must be clean
just lint
# or: cargo clippy --workspace --all-targets -- -D warnings

# 4. Run tests with the circuit-breaker feature enabled
cargo test -p prompt-hub --features circuit-breaker test_circuit_breaker_accessor
cargo test -p prompt-hub --features circuit-breaker -- circuit_breaker::tests

# 5. Test default build (circuit_breaker module should NOT compile in default)
cargo test -p prompt-hub test_circuit_breaker_accessor 2>&1 | grep -i "unresolved\|cannot find\|cfg.*not met"

# 6. Full workspace test (all-features)
just test
```

---

## 7. Acceptance Criteria

- [ ] `cargo check` (default features) passes green — no new errors.
- [ ] `cargo check --all-features` passes green — circuit-breaker path compiles.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes clean.
- [ ] `cargo fmt --all` produces no diffs.
- [ ] Integration test `test_circuit_breaker_accessor` passes under `--features circuit-breaker`.
- [ ] All 9 pre-existing `circuit_breaker::tests` unit tests pass under `--features circuit-breaker`.
- [ ] `PromptHub` struct contains an `Arc<CircuitBreaker>` field (visible in code).
- [ ] `hub.circuit_breaker()` returns `Arc<CircuitBreaker>` and is callable.
- [ ] No `circuit_breaker` types appear in default-feature builds (cfg-gated everywhere).

---

## 8. Drift Flagged

**None.** The circuit_breaker module is fully Rust-native: `#![forbid(unsafe_code)]`, native `async fn in trait` (not used here — it's sync), standard `Arc<RwLock<...>>` for interior mutability, `HubError` for error handling, `tracing` for logging. No non-Rust patterns detected.

---

## 9. Change Order (leaf-first)

1. **Import** — line ~15 in hub.rs (add use statement with cfg gate)
2. **Struct field** — line 138 in hub.rs (add field with cfg gate)
3. **Constructor init** — line 199 in hub.rs (add Arc::new with cfg gate)
4. **Info log** — line 206 in hub.rs (add logging with cfg gate)
5. **Accessor method** — after line 1460 in hub.rs (pub fn with cfg gate)
6. **Integration test** — end of `mod tests` in hub.rs

This order ensures the struct field is declared before init, init before accessor, and the test only compiles once everything is wired.

---

## 10. Future Consumers (design notes for wiring-gate)

The circuit breaker is designed as a low-level utility that other modules will consume:
- **provider_health** (already wired): could wrap provider health probe calls in a CB to avoid cascading failures when a provider goes down.
- **load_balancer** (already wired): could gate `record_lb_failure()` and `select_provider()` behind per-provider circuit breakers.
- **future external call modules**: any module that makes HTTP/fork/external calls should wrap them in the CB.

No accessor delegation methods are needed right now — the raw `Arc<CircuitBreaker>` handle gives full access to `call()`, `current_state()`, and `reset()` to any consumer. Wrappers can be added later if a simpler API is desired for specific use cases.
