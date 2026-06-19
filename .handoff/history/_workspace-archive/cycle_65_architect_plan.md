# Cycle 65 Architect Plan — `sandbox` Feature

## Summary

Add a **P1 security feature** to prompt_hub: a per-prompt execution sandbox that defines resource bounds, rate limits, and isolation policies. This is a *configuration + enforcement layer* on top of existing PromptHub operations — not a process-level sandbox (which would require unsafe code). The sandbox gates/limits what PromptHub allows each prompt to consume.

## 1. Blast Radius & Risk Assessment

### Files touched
| File | Change type | Risk |
|------|------------|------|
| `prompt-hub/src/lib.rs` | Add `#[cfg(feature = "sandbox")] pub mod sandbox;` gate | **Low** — one line, no existing callers impacted |
| `prompt-hub/Cargo.toml` | Remove `sandbox` from "Dead features" comment; add `sandbox = []` to real modules list (Category C) | **Low** — purely additive feature flag |
| `prompt-hub/src/models.rs` | Add 4 new types + 1 new enum (no field changes on existing types) | **Low** — append-only, no breaking changes |
| `prompt-hub/src/error.rs` | Append `SecurityViolation(String)` variant to `HubError` | **Medium** — thiserror enum change requires all match sites; however `HubError` variants are only constructed via `HubError::X(…)` or `thiserror` derive, and no code currently exhaustively matches on it (the enum is never matched exhaustively anywhere in the codebase) |
| `prompt-hub/src/hub.rs` | Add field + wiring + public methods behind feature gate | **Medium** — adds fields to `PromptHub` struct and ~8 new methods; guarded by `#[cfg(feature = "sandbox")]` so default-build is unaffected |

### Caller/impact analysis
- **Zero existing callers** of sandbox types (they don't exist yet). This is a green-field feature.
- `HubError::SecurityViolation` variant: `thiserror::Error` derive means adding a field variant does not break compilation at call sites — it's a new enum variant, and `#[error("...")]` attributes handle formatting. The only risk is if code exhaustively pattern-matches on `HubError`, which none does (verified by grep for `match.*HubError` or `HubError::`).
- **Risk classification: Low.** This is the safest kind of change — purely additive, feature-gated, no type changes to existing public APIs.

## 2. Rust-Native Design Decisions

### Feature gate
- **Name:** `sandbox` (kebab-case in Cargo.toml, snake_case module `sandbox.rs`)
- **Category:** Real module (Category C) — moves from "Dead features" comment into the actual `[features]` list with `sandbox = []`
- **No optional dependencies.** Everything uses existing workspace deps: `serde`, `thiserror`, `tracing`, `uuid`, `chrono`, `std::sync`.

### Architecture philosophy
The sandbox is a **policy configuration + in-process enforcement layer**, not process isolation. Since this codebase runs inside a single Rust process (no subprocess spawning) and has `#![forbid(unsafe_code)]`:

- "Memory limits" = configured max tokens/bytes per prompt evaluation (tracked via a budget counter, enforced before the operation proceeds)
- "CPU/time limits" = timeout enforcement via `tokio::timeout` on async operations
- "Network isolation" = a deny-all flag that blocks external network calls from within the sandbox context
- Rate limiting = per-sandbox request quota with sliding-window counters

### Core types (models.rs additions)

```rust
/// Sandbox mode: what level of restriction applies to this prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxMode {
    /// No sandboxing — full access.
    Unrestricted,
    /// Resource-bounded execution with configurable limits.
    Bounded(SandboxConfig),
    /// Full isolation: deny all network, strict token/cost caps.
    Isolated(SandboxConfig),
}

impl Default for SandboxMode {
    fn default() -> Self {
        Self::Unrestricted
    }
}

/// Resource limits enforced by the sandbox engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub max_tokens: u32,
    pub max_cost_usd: f64,
    pub timeout_secs: u64,
    pub max_memory_bytes: u64,
    pub rate_limit_per_min: u32,
    pub deny_network: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_tokens: 8192,
            max_cost_usd: 10.0,
            timeout_secs: 60,
            max_memory_bytes: 268_435_456, // 256 MB
            rate_limit_per_min: 60,
            deny_network: true,
        }
    }
}

/// A named sandbox instance bound to a prompt or user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sandbox {
    pub id: Uuid,
    pub name: String,
    pub mode: SandboxMode,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "default".to_string(),
            mode: SandboxMode::default(),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

/// Result of a sandbox enforcement check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxCheckResult {
    Allowed,
    RateLimited { retry_after_secs: u64 },
    BudgetExceeded { spent_usd: f64, limit_usd: f64 },
    TokenLimitExceeded { used: u32, max: u32 },
    Timeout { elapsed_secs: u64 },
    NetworkDenied,
}
```

### Engine design (sandbox.rs)

The sandbox engine provides:

1. **`SandboxEngine`** — the main struct that owns all sandbox definitions and a rate-limit counter per-sandbox. Thread-safe via `Arc<Mutex<>>`.
2. **`check(&self, sandbox_id, prompt_tokens, cost_usd)`** — returns `SandboxCheckResult`, gate before execution.
3. **`apply_timeout<T>(&self, future: impl Future<Output = T>) -> impl Future<Output = Result<T, HubError>>`** — wraps any future with the configured timeout.
4. **CRUD methods** — `create_sandbox()`, `update_sandbox()`, `delete_sandbox()`, `get_sandbox()` (standard pattern matching `CostLimiter`).

### hub.rs wiring pattern

Following the `cost-limits` and `circuit-breaker` pattern exactly:

```rust
#[cfg(feature = "sandbox")]
sandbox_engine: std::sync::Arc<crate::sandbox::SandboxEngine>,
```

In `new()`:
```rust
#[cfg(feature = "sandbox")]
sandbox_engine: std::sync::Arc::new(crate::sandbox::SandboxEngine::default()),
```

Plus ~5 public methods on `PromptHub` exposed behind `#[cfg(feature = "sandbox")]`:
- `create_sandbox()` → forwards to engine
- `get_sandbox()` → forwards
- `delete_sandbox()` → forwards
- `check_sandbox(prompt_id, tokens, cost)` → enforcement point
- `apply_timeout(&self, duration, future)` → async wrapper

## 3. Files & Changes (exact)

### File 1: `prompt-hub/Cargo.toml` (line ~59, the "Stub features" section)

**Add after line with `preview = []`:**
```yaml
# ---------------------------------------------------------------------------
# Real modules with code (Category C): kept as gated features. No optional
# deps required — these are pure Rust with existing std/deps already present.
# ---------------------------------------------------------------------------
quota = []
retention = []
gradual-rollout = []
circuit-breaker = []
moderation = []
budget = []
analytics = []
preview = []
i18n = []
sandbox = []              # <-- ADD THIS LINE
```

**Remove `sandbox` from the "Dead features" comment block (line ~69):**
Delete `sandbox` from the dead-features list:
```
#   malware-scan, multi-provider, offline, qdrant, sandbox, voice-anonymize, ...
```
becomes:
```
#   malware-scan, multi-provider, offline, qdrant, voice-anonymize, ...
```

### File 2: `prompt-hub/src/lib.rs` (after line 78 `pub mod quota;`)

**Add the module gate (line ~80, between `quota` and `retention`):**
```rust
#[cfg(feature = "sandbox")]
pub mod sandbox;
```

### File 3: `prompt-hub/src/models.rs` (append before the test block at line 987)

**Add new types after line 986 (before `mod model_tests`):**
- `SandboxMode` enum (~20 lines)
- `SandboxConfig` struct + impl Default (~15 lines)
- `Sandbox` struct + impl Default (~15 lines)
- `SandboxCheckResult` enum (~10 lines)

**Add model tests after existing tests (after line 1156):**
- `test_sandbox_mode_default` → verifies `Unrestricted` is default
- `test_sandbox_config_default` → verifies sane defaults
- `test_sandbox_check_result_variants` → verifies enum variants exist

### File 4: `prompt-hub/src/error.rs` (before the closing `}` of the `HubError` enum)

**Add new variant:**
```rust
#[error("Security violation: {0}")]
SecurityViolation(String),
```

Verify this does not conflict with any existing variant name. Based on the grep above, the existing variants are: `Internal`, `InvalidInput`, `NotFound`, `Unauthorized`, `Conflict`, `RateLimited`, `Timeout`, `Validation`, `ValidationError`, `BadRequest`, `AuthError`, `AuditError`, `LockError`, `StorageError`, `SearchError`, `SerdeError`, `SyncError`, `SanitizationError`, `Io`, `Serialization`, `Network`, `Database`, `Plugin`, `Security`, `FallbackExhausted`. **No existing `SecurityViolation`** — this is safe to add.

### File 5: `prompt-hub/src/hub.rs`

#### 5a. Imports (after line 38 `use crate::quota::QuotaEnforcer;`)

```rust
#[cfg(feature = "sandbox")]
use crate::sandbox::{Sandbox, SandboxConfig, SandboxMode};
```

#### 5b. Struct field (after line 181 `preview_engine`, before `analytics`)

```rust
    #[cfg(feature = "sandbox")]
    sandbox_engine: std::sync::Arc<crate::sandbox::SandboxEngine>,
```

#### 5c. Initialization in `new()` (after line 274 `preview_engine` init)

```rust
            #[cfg(feature = "sandbox")]
            sandbox_engine: std::sync::Arc::new(crate::sandbox::SandboxEngine::default()),
```

#### 5d. Post-struct info log (after line 317 `preview engine ready`)

```rust
        #[cfg(feature = "sandbox")]
        info!("Sandbox engine initialized in permissive mode");
```

#### 5e. Public methods (append after existing public methods, ~line 1800+)

Add behind `#[cfg(feature = "sandbox")]` block:
- `create_sandbox(config)` → `Result<Sandbox>`
- `get_sandbox(id)` → `Result<Sandbox>`
- `update_sandbox(id, config)` → `Result<Sandbox>`
- `delete_sandbox(id)` → `Result<()>`
- `check_sandbox(prompt_id, tokens, cost_usd)` → `Result<SandboxCheckResult>`
- `apply_timeout(duration, future)` → `impl Future<Output = Result<T, HubError>>`

### File 6: NEW — `prompt-hub/src/sandbox.rs` (new file, ~250 lines)

Contents:
- Module doc comment with `#![forbid(unsafe_code)]`
- `SandboxConfig` + `SandboxCheckResult` types (re-exported for engine use)
- `RateWindow` — a simple per-second counter struct for rate limiting
- `SandboxStore` — the in-memory store backed by `Arc<Mutex<Vec<...>>>`
- `SandboxEngine` — main struct with CRUD + check + timeout methods
- Full test module with 8+ tests

## 4. Migrations

**None required.** The sandbox feature operates entirely as an in-process configuration layer. No database schema changes are needed:
- Sandbox definitions live in memory (`Arc<Mutex<Vec<Sandbox>>>`) within the `PromptHub` struct
- There's no persistence requirement for sandbox configs at this P1 scope (they're ephemeral per-instance)
- If persistence is needed later, it can be added as a separate migration + storage hook without breaking P1

## 5. Test Plan

### Unit tests in `sandbox.rs` (`#[cfg(test)] mod sandbox_tests`)

| Test name | What it verifies |
|-----------|-----------------|
| `test_sandbox_create_default` | Creating a sandbox with default config succeeds, id is Uuid v4 |
| `test_sandbox_create_isolated` | Creating an isolated sandbox with deny_network=true |
| `test_sandbox_get_nonexistent` | Returns `HubError::NotFound` for missing ID |
| `test_sandbox_update` | Modifying limits after creation reflects in get() |
| `test_sandbox_delete` | Delete removes it; subsequent get returns NotFound |
| `test_sandbox_check_allows_under_limits` | Tokens and cost under limits → `Allowed` |
| `test_sandbox_check_token_limit` | Exceeding max_tokens → `TokenLimitExceeded` |
| `test_sandbox_check_cost_limit` | Exceeding max_cost_usd → `BudgetExceeded` |
| `test_sandbox_rate_limit_exhausted` | Exceeding rate_limit_per_min → `RateLimited` |
| `test_sandbox_isolation_denies_network` | Mode Isolated with network call attempt → `NetworkDenied` |

### Hub-level integration test (`prompt-hub/src/sandbox.rs` or separate integration file)

| Test name | What it verifies |
|-----------|-----------------|
| `test_hub_create_and_check_sandbox` | Full flow: create on hub → check enforcement returns result |

## 6. Verify Commands

```bash
# Check default build (sandbox OFF — must compile clean)
rtk cargo check --workspace

# Check with sandbox feature ON
rtk cargo check --workspace --features sandbox

# Clippy across all features (including sandbox)
just lint

# Format check
just fmt

# Run sandbox unit tests
rtk cargo test -p prompt-hub sandbox

# Run full test suite with sandbox enabled
rtk cargo test --workspace --all-features
```

## 7. Acceptance Criteria

1. [ ] `sandbox` feature flag exists in `Cargo.toml` under the "Real modules" list (Category C), **not** in the dead features comment
2. [ ] `sandbox` removed from the dead features comment block in `Cargo.toml`
3. [ ] `prompt-hub/src/lib.rs` declares `#[cfg(feature = "sandbox")] pub mod sandbox;`
4. [ ] `prompt-hub/src/sandbox.rs` exists with `#![forbid(unsafe_code)]` at top
5. [ ] All 4 new types (`SandboxMode`, `SandboxConfig`, `Sandbox`, `SandboxCheckResult`) are defined in `models.rs` and derive `Serialize + Deserialize`
6. [ ] `HubError::SecurityViolation` variant exists with `#[error("Security violation: {0}")]` formatting
7. [ ] `PromptHub` struct has a `sandbox_engine` field behind `#[cfg(feature = "sandbox")]`
8. [ ] `PromptHub::new()` initializes `sandbox_engine` behind `#[cfg(feature = "sandbox")]`
9. [ ] `cargo check --workspace` (default features, sandbox OFF) compiles clean — no dead-code warnings that would break CI
10. [ ] `cargo check --workspace --features sandbox` compiles clean
11. [ ] `just lint` (clippy -D warnings) passes with both default and `--all-features`
12. [ ] 8+ unit tests in `sandbox.rs` all pass (`cargo test -p prompt-hub sandbox`)
13. [ ] Hub integration test passes: create → check → enforce flow works end-to-end
14. [ ] `SandboxCheckResult` enum variants correctly map to enforcement decisions (no unreachable arms)
15. [ ] All public types are re-exported or accessible via `crate::sandbox::` paths

## 8. Drift Flagged

| Backlog description | Rust-native translation | Status |
|---------------------|------------------------|--------|
| "memory/CPU limits" | Configured token/cost budgets tracked in-process + `tokio::timeout`; NOT actual process memory tracking (requires unsafe) | **FLAGGED — design adapted** |
| "network isolation" | `deny_network` flag on `SandboxConfig` that blocks network operations from sandboxed context; NOT actual iptables/namespace isolation | **FLAGGED — design adapted** |
| "rate-limiting" | In-process per-sandbox sliding-window counter using `std::sync::Mutex<HashMap<String, Vec<u64>>>` (timestamps of past requests) | **Adapted to Rust-native** |
| "per-prompt execution sandbox" | Per-prompt configuration entry in `SandboxEngine`, not a process-level sandbox. The enforcement happens at the PromptHub API layer, blocking operations before they execute. | **FLAGGED — design adapted** |

**Key risk:** The backlog description reads like a process isolation feature (OS-level). This design deliberately scopes it to a *configuration + enforcement policy engine* because:
- `#![forbid(unsafe_code)]` forbids ptrace/fork/mmap tricks
- No subprocess spawning in this codebase anyway
- The prompt_hub architecture routes all work through async Rust, not system calls

## 9. Implementation Order (leaf-first)

1. **`prompt-hub/src/error.rs`** — Add `SecurityViolation` variant (1 line, no callers yet)
2. **`prompt-hub/src/models.rs`** — Append 4 new types + model tests (~80 lines)
3. **`prompt-hub/src/sandbox.rs`** — Create new file with engine + tests (~250 lines)
4. **`prompt-hub/Cargo.toml`** — Move `sandbox` from dead features to real features list
5. **`prompt-hub/src/lib.rs`** — Add module gate line
6. **`prompt-hub/src/hub.rs`** — Add struct field, init, info log, public methods (~30 lines)

This order ensures each step compiles: error types first (no deps), models second (no deps on sandbox), engine third (depends on models), then gates/wiring last.
