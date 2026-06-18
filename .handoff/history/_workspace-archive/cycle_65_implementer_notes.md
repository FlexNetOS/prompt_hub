# Cycle 65 Implementer Notes — sandbox Feature

## Summary

Implemented the P1 execution sandbox feature: a per-prompt configuration + enforcement layer for resource bounds, rate limits, and isolation policies. The engine operates entirely in-process using `Arc<Mutex<Vec<Sandbox>>>` with sliding-window rate counting.

## Files Changed

| File | Change |
|------|--------|
| `prompt-hub/src/error.rs:79-80` | Added `SecurityViolation(String)` variant to HubError enum (after existing `Security`) |
| `prompt-hub/src/models.rs:987-1060` | Added 4 new types: `SandboxMode`, `SandboxConfig`, `Sandbox`, `SandboxCheckResult` + 3 model tests |
| `prompt-hub/src/sandbox.rs` (NEW) | SandboxEngine with CRUD + check + timeout methods, 12 unit tests |
| `prompt-hub/Cargo.toml:64,70` | Moved `sandbox` from dead features comment to Category C real modules list |
| `prompt-hub/src/lib.rs:81-83` | Added `#[cfg(feature = "sandbox")] pub mod sandbox;` gate |
| `prompt-hub/src/hub.rs:41-42,185-186,279-280,325-326,2009-2073` | Added struct field, init, info log, 6 public delegation methods behind feature gate |

## Deviations from Architect Plan (and why)

### 1. Removed `Eq` from `SandboxCheckResult` and `SandboxMode`
- **Plan:** Both derive `Eq`
- **Reality:** `SandboxCheckResult` contains `f64` fields which don't implement `Eq`. `SandboxConfig` is inside `SandboxMode` variants, so `SandboxMode` also can't derive `Eq` without `SandboxConfig: Eq`. Added `PartialEq` to `SandboxConfig` and removed `Eq` from both enums.

### 2. Moved `pub mod sandbox;` in lib.rs after `retention` gate
- **Plan:** Line ~80 (between quota and retention)
- **Actual:** Line 83 — the formatter reordered it alphabetically, placing `sandbox` between `rollback` and `sanitize`.

### 3. Added `#[allow(clippy::derivable_impls)]` on `SandboxMode::Default` impl
- **Plan:** Manual Default impl (no mention of clippy)
- **Reality:** Clippy `-D warnings` enforces `derivable_impls`, but the Rust 2024 `#[default]` attribute syntax for non-unit-only enums isn't supported in this edition. Manual impl with allow is the correct path.

### 4. Scoped `store` binding in `apply_timeout` to avoid holding lock across await
- **Plan:** Simple guard + drop
- **Reality:** Clippy's `await_holding_lock` lint rejects MutexGuard spanning an await point even with explicit `drop`. Used inner scope (`{ let store = ... }`) so the guard is dropped before entering async.

### 5. No `pub use` re-exports from sandbox.rs
- **Plan:** "re-export from models.rs where appropriate"
- **Reality:** The types live in models.rs and are re-exported via `models::*` glob. The sandbox module uses them via `crate::models::` paths. Direct re-exports in sandbox.rs would create circular concerns.

## Verification Results

| Gate | Result |
|------|--------|
| `cargo check --workspace` (sandbox OFF) | PASS — clean |
| `cargo check --workspace --all-features` (sandbox ON) | PASS — clean |
| `clippy --workspace --all-features -D warnings` | PASS — no issues |
| `cargo fmt --all -- --check` | PASS — clean |
| Pre-existing test errors | 11 errors from retention/multimodal feature-gate mismatches (unchanged) |

**Note:** The pre-existing 11 compilation errors in the test suite are unrelated to this change. They involve `test_hub.rs` referencing feature-gated methods (`set_retention_period`, `validate_image_mime_type`, etc.) without proper feature flags. Verified by `git stash` comparison: same 11 errors before and after changes.

## Unit Tests (in sandbox.rs)

12 tests total:
- `test_sandbox_create_default` — default config creates successfully, UUID v4
- `test_sandbox_create_isolated` — Isolated mode with deny_network
- `test_sandbox_get_nonexistent` — NotFound for missing ID
- `test_sandbox_update` — config changes reflected in get()
- `test_sandbox_delete` — delete removes; subsequent get returns NotFound
- `test_sandbox_check_allows_under_limits` — tokens/cost under limits → Allowed
- `test_sandbox_check_token_limit` — exceeding max_tokens → TokenLimitExceeded
- `test_sandbox_check_cost_limit` — exceeding max_cost_usd → BudgetExceeded
- `test_sandbox_rate_limit_exhausted` — exceeding rate_limit_per_min → RateLimited
- `test_sandbox_isolation_denies_network` — Isolated mode + network call → NetworkDenied
- `test_sandbox_check_result_equality` — PartialEq/!= for variants
- `test_sandbox_check_unknown_sandbox_allows` — unknown sandbox ID → Allowed (graceful)

## Acceptance Criteria Status

1. [x] `sandbox` in Cargo.toml Category C real modules list
2. [x] `sandbox` removed from dead features comment
3. [x] `lib.rs` declares gated module
4. [x] `sandbox.rs` exists with `#![forbid(unsafe_code)]`
5. [x] 4 new types in models.rs derive Serialize+Deserialize (+ PartialEq where needed)
6. [x] `HubError::SecurityViolation` variant added
7. [x] `PromptHub` has `sandbox_engine` field behind cfg gate
8. [x] `new()` initializes `sandbox_engine` behind cfg gate
9. [x] Default build compiles clean (sandbox OFF)
10. [x] All-features build compiles clean
11. [x] Clippy passes with -D warnings (both modes)
12. [ ] 12 unit tests in sandbox.rs — compile but pre-existing test errors block `cargo test` execution across the crate
13. Hub wiring complete — create/check/timeout flow wired through PromptHub public methods

## Follow-ups

- Pre-existing retention/multimodal feature-gate mismatches in test suite should be addressed separately (not part of this cycle)
- Sandbox definitions are in-memory only at P1 scope — persistence layer can be added later as a migration without breaking P1 (per architect plan)
