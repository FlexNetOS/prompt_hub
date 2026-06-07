# Cycle 2 — Architect Plan: Wire `quality_gate::QualityGate` into PromptHub

## 1. Blast Radius

| Symbol | Callers (current) | Risk |
|--------|-------------------|------|
| `QualityGate` (quality_gate.rs) | 0 production callers, only self-tests | **Low** — new addition |
| `QualityGate::check()` | 0 callers | **Low** |
| `QualityGate::check_minimal()` | 0 callers | **Low** |
| `Linter`, `SecurityScanner`, `PerformanceChecker`, `AccessibilityChecker` traits | 0 implementations outside tests | **Low** |

The module (`pub mod quality_gate`) is already declared in lib.rs (line 45) and re-exports nothing yet. This is a greenfield wiring — no existing callers to update.

## 2. Design Decisions (Rust-Native)

### Where logic lives
All logic stays inside `quality_gate.rs` (already correct). hub.rs adds **one field** (`Arc<QualityGate>`) and **one method** (`run_quality_gate()`) that constructs the gate, runs it, and returns a `QualityResult`. No trait impls, no feature gates.

### Async style
- Native `async fn in trait` (already Rust 2024 Edition). The gate's `.check()` uses boxed-future methods on the trait objects internally — hub.rs just `.await`s `gate.check(&artifact)` directly.
- Return type: `Result<QualityResult>` via `crate::error::Result`.

### Module import
- `use crate::quality_gate::{QualityGate, QualityResult};` at the top of hub.rs (near existing imports).

### Struct field addition
```rust
pub struct PromptHub {
    // ... existing fields ...
    quality_gate: Arc<QualityGate>,
}
```

### Method signature
```rust
/// Run the quality gate pipeline against an artifact.
#[instrument(skip(self))]
pub async fn run_quality_gate(&self, artifact: &Artifact) -> Result<QualityResult> {
    let result = self.quality_gate.check(artifact).await?;
    info!(
        passed = %result.passed,
        warnings = %result.warnings.len(),
        errors = %result.errors.len(),
        "Quality gate result"
    );
    Ok(result)
}
```

No checkers are registered by default (empty gate always passes via `check_minimal`-style behavior). This is intentional: checkers are added downstream when real implementations exist. The hub exposes the capability; wiring of specific checkers can happen later via a builder pattern or direct mutation on the inner `Arc`.

### Drift detected
None. `quality_gate.rs` uses native async traits with boxed-future object-safety patterns — fully Rust-native. No `async_trait`, no `unsafe`, no panics-as-errors.

## 3. Files & Changes

### File 1: `prompt-hub/src/hub.rs`

**Change A — Add import (after line 18, before the struct):**
```rust
use crate::quality_gate::{QualityGate, QualityResult};
```

**Change B — Add field to `PromptHub` struct (line ~102, after `hooks`):**
```rust
    quality_gate: Arc<QualityGate>,
```

**Change C — Initialize in `new()` (after line ~137, before the closing brace of `Self { ... }`):**
```rust
            quality_gate: Arc::new(QualityGate::new()),
```

**Change D — Add method to `impl PromptHub` block (after `learn_from_feedback`, inside the impl, before line 638):**
```rust
    // ── Quality gate ────────────────────────────────────────────────────

    /// Run the quality gate pipeline against an artifact.
    ///
    /// Returns a `QualityResult` with scores and pass/fail for lint,
    /// security, performance, and accessibility checks registered on the gate.
    #[instrument(skip(self))]
    pub async fn run_quality_gate(&self, artifact: &Artifact) -> Result<QualityResult> {
        let result = self.quality_gate.check(artifact).await?;
        info!(
            passed = %result.passed,
            warnings = %result.warnings.len(),
            errors = %result.errors.len(),
            "Quality gate result"
        );
        Ok(result)
    }
```

## 4. Tests

**Inline test in hub.rs `mod tests` (after existing tests):**

```rust
    #[tokio::test]
    async fn test_quality_gate_empty_passes() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let artifact = Artifact::Code {
            path: "test.rs".to_string(),
            content: "fn main() {}".to_string(),
            language: "rust".to_string(),
        };

        let result = hub.run_quality_gate(&artifact).await.unwrap();
        assert!(result.passed);
        assert_eq!(result.lint_score, 1.0);
        assert_eq!(result.security_score, 1.0);
        assert_eq!(result.performance_score, 1.0);
        assert_eq!(result.accessibility_score, 1.0);
    }

    #[tokio::test]
    async fn test_quality_gate_result_type() {
        let dir = TempDir::new().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), test_config())
            .await
            .unwrap();

        let artifact = Artifact::Prompt {
            content: "You are a helpful assistant.".to_string(),
        };

        let result = hub.run_quality_gate(&artifact).await.unwrap();
        assert!(result.passed);
        assert!(result.warnings.is_empty());
        assert!(result.errors.is_empty());
    }
```

These tests verify the wired method returns correct `QualityResult` values from the empty gate (always passes, all scores = 1.0).

## 5. Verify Commands

```bash
# 1. Check default features compile
just check

# 2. Clippy clean
just lint

# 3. Run only the new tests
cargo test -p prompt-hub --lib quality_gate -- --test-threads=1

# 4. Full workspace test
just test
```

## 6. Acceptance Criteria

- [ ] `just check` — default features compile with zero errors
- [ ] `just lint` — clippy `-D warnings` clean (zero warnings)
- [ ] `cargo test -p prompt-hub --lib quality_gate` — both new inline tests pass
- [ ] `just test` — all workspace tests still pass (no regression)
- [ ] `run_quality_gate` method exists on `PromptHub` with signature `async fn(&self, &Artifact) -> Result<QualityResult>`
- [ ] QualityGate field is `Arc<QualityGate>` initialized in `new()` with `QualityGate::new()`
- [ ] No new dependencies added (uses only existing crate deps)

## 7. Drift Flagged

None. The quality_gate module uses native Rust 2024 async traits with boxed-future object-safety — no `async_trait`, no `unsafe`, no panics-as-errors, no feature gates needed.
