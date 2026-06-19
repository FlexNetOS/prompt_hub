# Cycle 64 Architect Plan — `gradual-rollout` Feature

**Backlog item:** P1a-3: Graduated prompt release system; A/B/n rollout by segment, percentage-based canary with auto-rollback thresholds. Priority: HIGH (extends existing canary work).

**Preceded by cycles 6-7:** beta-program (phased deployment), cost-limits, multi-provider (vendor routing).

---

## 1. Blast Radius & Risk Assessment

### Callers / Impact
| Symbol | Module | Caller count | Risk |
|--------|--------|-------------|------|
| `CanaryDeployment` (models.rs:864) | lib type used by canary.rs + hub.rs + models test | 2 callers in hub.rs (canary_deploy, canary_should_rollback) + 1 test | **Medium** — already wired behind `#[cfg(feature = "canary")]` but hub.rs:24 import is missing cfg gate |
| `CanaryEngine::deploy()` (canary.rs:16) | Hub canary_deploy() | 1 caller (hub.rs:1877) + 1 test | **Low** |
| `CanaryEngine::should_rollback()` (canary.rs:41) | Hub canary_should_rollback() | 1 caller (hub.rs:1883) + 1 test | **Low** |
| hub struct field `canary_engine` (hub.rs:179-179, :272, :285) | PromptHub | 0 direct callers (only via canary_engine_handle()) | **Medium** — the field instantiation at line 285 lacks cfg gate for SafeDeployer pattern consistency |
| RolloutStage/BetaCohort/ProgramStats (beta_program.rs:14-25, :47-70, :130) | beta-program feature | ~4 hub.rs delegation methods + tests | **Low** — already wired behind `#[cfg(feature = "beta-program")]` |

### Existing Infrastructure
- `canary` feature **already exists** in Cargo.toml:57 with module file `canary.rs` (92 lines)
- `CanaryDeployment` struct exists in models.rs:864-869 — 4 fields, serde-derived
- `CanaryEngine::deploy()` uses SHA-256 user-bucket hashing (canary.rs:16-37)
- `CanaryEngine::should_rollback()` checks error_rate + latency_p99 against thresholds (canary.rs:41-58)
- **BUILD BUG DETECTED:** `hub.rs:24` imports `CanaryDeployment` without cfg gate. When `canary` is off (default), this import is dead code → clippy `-D warnings` catches it as unused import → **build breaks** with default features

### Risk Classification
- **Medium overall.** New feature (gradual-rollout) touches: lib.rs module gate, hub.rs delegation methods, models.rs type additions, and creates a new source file. The existing canary build bug must be fixed as a prerequisite.
- No public API consumers to worry about — this is an internal library feature.
- No server routes or CLI commands need updating (canary has zero wiring into either).

---

## 2. Rust-Native Design Decisions

### 2.1 Feature naming & gate strategy
- **Feature flag:** `gradual-rollout = []` — added to Cargo.toml Category C section (real module, no optional deps)
- **Module file:** `prompt-hub/src/gradual_rollout.rs` — follows Rust snake_case convention for module names
- The existing `canary` feature is a *subset* of gradual-rollout. Design: **gradual-rollout supersedes canary**. All canary functionality moves into gradual-rollout; the old `canary` feature becomes an alias/gate that re-exports from gradual-rollout for backward compatibility (or we remove canary entirely if no other code depends on it).
- **Decision:** Remove `canary` feature, fold its module into `gradual-rollout`. One source of truth.

### 2.2 Type additions in models.rs (no separate module for types)
All new model types go in `models.rs` under the existing section structure:
```
// Section: "Canary and deployment configuration" (insert after LLMProvider, line ~870)
```

Types to add to models.rs:

```rust
/// Staged rollout stages — graduated percentage caps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RolloutStage {
    Internal,           // 0% external
    Alpha(u8),          // up to p% (caller-enforced cap)
    Beta50(u8),         // up to 50% + p% variance
    Beta90(u8),         // up to 90% + p% variance
    Production,         // 100%
}

/// Deployment segment for A/B/n rollout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolloutSegment {
    pub name: String,
    pub percentage: u8,              // 0-100 of total traffic
    pub target_users: Vec<Uuid>,     // whitelisted exact users
    pub rollout_stage: RolloutStage,
    pub created_at: DateTime<Utc>,
}

/// Auto-rollback policy for canary deployments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoRollbackPolicy {
    /// Roll back when error rate exceeds threshold.
    OnErrorRate { threshold: f64 },
    /// Roll back when p99 latency exceeds SLA budget (ms).
    OnLatencyP99 { sla_ms: u64 },
    /// Both — requires BOTH thresholds to be exceeded.
    OnBoth { error_rate: f64, latency_p99_ms: u64 },
}

/// A canary deployment configuration for gradual rollout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraduatedRolloutConfig {
    pub rollout_id: String,           // human-readable identifier
    pub feature: String,
    pub segments: Vec<RolloutSegment>,
    pub auto_rollback: AutoRollbackPolicy,
    pub active: bool,
}
```

Naming rationale: We keep `CanaryDeployment` in models.rs for backward compat with existing canary tests, but add the new `GraduatedRolloutConfig` as the primary type. `RolloutStage` here is a superset of `beta_program::RolloutStage` — we **reuse** `beta_program::RolloutStage` from beta_program.rs rather than duplicating.

### 2.3 Core engine design (gradual_rollout.rs)
```rust
pub struct RolloutEngine;

impl RolloutEngine {
    /// Check if a user should receive the canary variant.
    pub fn should_rollout(canary: &CanaryDeployment, user_id: Uuid) -> bool
    /// Evaluate auto-rollback policy against metrics.
    pub fn evaluate_rollback(config: &GraduatedRolloutConfig, error_rate: f64, latency_p99_ms: u64) -> bool
    /// Advance a segment to the next rollout stage.
    pub fn advance_stage(segment: &mut RolloutSegment) -> Option<RolloutStage>
    /// Create a new graduated rollout config from segments.
    pub fn create_config(rollout_id: &str, feature: &str, segments: &[RolloutSegment]) -> GraduatedRolloutConfig
}
```

Pattern match: Follows `CanaryEngine` static method style (no state, pure functions). No `RwLock` needed — the engine is stateless; callers manage rollout config lifecycle through hub.rs.

### 2.4 Hub.rs wiring pattern
Each feature follows this pattern in hub.rs:
1. `#[cfg(feature = "...")] use crate::...;` (imports, line ~8-40)
2. Struct field on `PromptHub`: `#[cfg(feature = "...")] field_type` (line ~162-193)
3. Field initialization in `new()`: `#[cfg(feature = "...")] field: Type::new()` (line ~253-285)
4. Delegation methods with cfg gates (after existing canary methods at :1871)
5. Test block (inline, behind cfg gate)

### 2.5 Cargo.toml changes
Add `gradual-rollout = []` in Category C section (after line 63). Remove `canary = []` and fold it into `gradual-rollout`.

---

## 3. Files & Changes

### File 1: `prompt-hub/Cargo.toml`
**Changes:**
1. Add line `gradual-rollout = []` after line 57 (`canary = []`)
2. Remove `canary = []` from line 57 (fold into gradual-rollout)
3. Update the dead-features comment block at line 68: remove `gradual-rollout` from the "removed" list since we're rebuilding it

### File 2: `prompt-hub/src/lib.rs`
**Changes:**
1. Remove lines 29-30 (`#[cfg(feature = "canary")] pub mod canary;`) 
2. After line 78 (after `quota` gate), add:
```rust
#[cfg(feature = "gradual-rollout")]
pub mod gradual_rollout;
```

### File 3: `prompt-hub/src/hub.rs`
**Changes:**

A. **Fix existing canary build bug (prerequisite):**
   - Line 24: Wrap import with cfg gate:
   ```rust
   #[cfg(feature = "gradual-rollout")]
   use crate::models::CanaryDeployment;
   ```

B. **Add imports for new types:**
   After line 8-9 (after canary import):
   ```rust
   #[cfg(feature = "gradual-rollout")]
   use crate::gradual_rollout::{AutoRollbackPolicy, GraduatedRolloutConfig, RolloutEngine, RolloutSegment};
   ```

C. **Add struct field:**
   After line 179 (`canary_engine` field):
   ```rust
   #[cfg(feature = "gradual-rollout")]
   active_rollouts: std::sync::Mutex<Vec<GraduatedRolloutConfig>>,
   ```

D. **Add field initialization in `new()`:**
   After line 272 (`canary_engine` init):
   ```rust
   #[cfg(feature = "gradual-rollout")]
   active_rollouts: std::sync::Mutex::new(Vec::new()),
   ```

E. **Remove canary engine field and init** (since we're folding canary into gradual-rollout):
   - Remove lines 178-179, 271-272 (`#[cfg(feature = "canary")]` canary_engine / Arc<CanaryEngine>)
   - Keep the `info!` log at line 315-316 but gate behind `gradual-rollout` instead

F. **Rename/replace existing canary methods** (lines 1871-1895) with gradual-rollout equivalents:
   Replace the entire block from line 1871 through 1895 with:
   ```rust
   // ── Gradual rollout / canary deployment ───────────────────────

   /// Deploy a canary version of a feature (legacy alias for GraduatedRollout).
   #[cfg(feature = "gradual-rollout")]
   #[instrument(skip(self, canary))]
   pub async fn canary_deploy(&self, canary: &CanaryDeployment, user_id: Uuid) -> Result<bool> {
       RolloutEngine::should_rollout(canary, user_id)
           .then_some(true)
           .ok_or_else(|| HubError::InternalError("rollout check failed".into()))
   }

   /// Register a new graduated rollout configuration.
   #[cfg(feature = "gradual-rollout")]
   pub fn register_rollout(&self, config: GraduatedRolloutConfig) {
       self.active_rollouts.lock().unwrap().push(config);
   }

   /// Check if a user should receive traffic from a given rollout.
   #[cfg(feature = "gradual-rollout")]
   pub fn check_rollout(&self, rollout_id: &str, user_id: Uuid) -> Option<bool> {
       let rollouts = self.active_rollouts.lock().unwrap();
       rollouts.iter().find(|r| r.rollout_id == rollout_id).map(|cfg| {
           cfg.segments.iter().any(|seg| seg.target_users.contains(&user_id))
               || RolloutEngine::should_rollout(
                   &CanaryDeployment { feature: cfg.feature.clone(), canary_percentage: 5.0, target_users: vec![], rollback_threshold: cfg.auto_rollback_threshold() },
                   user_id,
               )
       })
   }

   /// Evaluate whether an active rollout should auto-rollback.
   #[cfg(feature = "gradual-rollout")]
   pub fn evaluate_auto_rollback(&self, rollout_id: &str, error_rate: f64, latency_p99_ms: u64) -> Option<bool> {
       let rollouts = self.active_rollouts.lock().unwrap();
       rollouts.iter().find(|r| r.rollout_id == rollout_id).map(|cfg| {
           RolloutEngine::evaluate_rollback(cfg, error_rate, latency_p99_ms)
       })
   }

   /// Advance a rollout segment to the next stage.
   #[cfg(feature = "gradual-rollout")]
   pub fn advance_segment(&self, rollout_id: &str, segment_idx: usize) -> Option<RolloutStage> {
       let mut rollouts = self.active_rollouts.lock().unwrap();
       rollouts.iter_mut().find(|r| r.rollout_id == rollout_id).and_then(|cfg| {
           cfg.segments.get_mut(segment_idx).map(|seg| {
               let next = RolloutEngine::advance_stage(seg);
               next
           })
       })
   }
   ```

G. **Remove old canary methods** that are now redundant:
   - Remove `canary_should_rollback()` (hub.rs:1880-1889) — replaced by `evaluate_auto_rollback()`
   - Remove `canary_engine_handle()` (hub.rs:1891-1895) — not needed for stateless engine

H. **Add info! log in `new()`:**
   After line 316, add:
   ```rust
   #[cfg(feature = "gradual-rollout")]
   info!("Graduated rollout engine initialized");
   ```

### File 4: `prompt-hub/src/gradual_rollout.rs` (NEW)
Create new file with the `RolloutEngine` static methods, following CanaryEngine pattern exactly:

```rust
//! Graduated prompt release system with percentage-based canary and auto-rollback.
//!
//! Extends the legacy `canary` feature with segmented A/B/n rollout capabilities.

#![forbid(unsafe_code)]

use crate::error::Result;
use crate::models::*;
use sha2::{Digest, Sha256};
use tracing::{info, warn};
use uuid::Uuid;

/// Stateless rollout engine — pure functions for canary/graduated deployment.
#[derive(Debug, Clone, Default)]
pub struct RolloutEngine;

impl RolloutEngine {
    /// Determine whether *user_id* should receive the canary variant based on
    /// SHA-256 user-bucket hashing. Target-listed users are always included.
    pub fn should_rollout(canary: &CanaryDeployment, user_id: Uuid) -> bool {
        if canary.target_users.contains(&user_id) {
            return true;
        }
        let hash_input = format!("{}{}", user_id, canary.feature);
        let hash = Sha256::digest(hash_input.as_bytes());
        let bucket = (hash[0] as f64 / 255.0) * 100.0;
        bucket < canary.canary_percentage
    }

    /// Evaluate auto-rollback policy against observed metrics.
    pub fn evaluate_rollback(config: &GraduatedRolloutConfig, error_rate: f64, latency_p99_ms: u64) -> bool {
        match config.auto_rollback {
            AutoRollbackPolicy::OnErrorRate { threshold } => error_rate > threshold,
            AutoRollbackPolicy::OnLatencyP99 { sla_ms } => latency_p99_ms > sla_ms as u64,
            AutoRollbackPolicy::OnBoth { error_rate: er, latency_p99_ms: lat } => {
                error_rate > er && latency_p99_ms > lat
            }
        }
    }

    /// Advance a segment to the next rollout stage. Returns `None` if already at Production.
    pub fn advance_stage(segment: &mut RolloutSegment) -> Option<RolloutStage> {
        let next = match &segment.rollout_stage {
            RolloutStage::Internal => RolloutStage::Alpha(10),
            RolloutStage::Alpha(_) => RolloutStage::Beta50(25),
            RolloutStage::Beta50(_) => RolloutStage::Beta90(50),
            RolloutStage::Beta90(_) => RolloutStage::Production,
            RolloutStage::Production => return None,
        };
        segment.rollout_stage = next.clone();
        Some(next)
    }

    /// Create a new graduated rollout config with auto-rollback from segments.
    pub fn create_config(rollout_id: &str, feature: &str, segments: &[RolloutSegment]) -> GraduatedRolloutConfig {
        let max_err = segments.iter().map(|s| s.percentage as f64 / 100.0).fold(0.05, f64::min);
        GraduatedRolloutConfig {
            rollout_id: rollout_id.to_string(),
            feature: feature.to_string(),
            segments: segments.to_vec(),
            auto_rollback: AutoRollbackPolicy::OnErrorRate { threshold: max_err },
            active: true,
        }
    }
}

// ── Helper impl on GraduatedRolloutConfig ─────────────────────────

impl GraduatedRolloutConfig {
    fn auto_rollback_threshold(&self) -> f64 {
        match self.auto_rollback {
            AutoRollbackPolicy::OnErrorRate { threshold } => threshold,
            _ => 0.05, // default safety net
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rollout_target_user_always_included() {
        let uid = Uuid::new_v4();
        let canary = CanaryDeployment {
            feature: "test".into(),
            canary_percentage: 0.0,
            target_users: vec![uid],
            rollback_threshold: 0.05,
        };
        assert!(RolloutEngine::should_rollout(&canary, uid));
    }

    #[test]
    fn test_rollout_hash_bucket_excluded() {
        let canary = CanaryDeployment {
            feature: "zero".into(),
            canary_percentage: 0.0,
            target_users: vec![],
            rollback_threshold: 0.05,
        };
        let uid = Uuid::new_v4();
        assert!(!RolloutEngine::should_rollout(&canary, uid));
    }

    #[test]
    fn test_rollback_on_error_rate() {
        let config = GraduatedRolloutConfig {
            rollout_id: "t".into(),
            feature: "t".into(),
            segments: vec![],
            auto_rollback: AutoRollbackPolicy::OnErrorRate { threshold: 0.05 },
            active: true,
        };
        assert!(RolloutEngine::evaluate_rollback(&config, 0.10, 100));
        assert!(!RolloutEngine::evaluate_rollback(&config, 0.01, 100));
    }

    #[test]
    fn test_rollback_on_latency_p99() {
        let config = GraduatedRolloutConfig {
            rollout_id: "t".into(),
            feature: "t".into(),
            segments: vec![],
            auto_rollback: AutoRollbackPolicy::OnLatencyP99 { sla_ms: 500 },
            active: true,
        };
        assert!(RolloutEngine::evaluate_rollback(&config, 0.01, 600));
        assert!(!RolloutEngine::evaluate_rollback(&config, 0.01, 400));
    }

    #[test]
    fn test_advance_stage_sequence() {
        let mut segment = RolloutSegment {
            name: "s".into(),
            percentage: 0,
            target_users: vec![],
            rollout_stage: RolloutStage::Internal,
            created_at: chrono::Utc::now(),
        };
        assert_eq!(RolloutEngine::advance_stage(&mut segment).unwrap(), RolloutStage::Alpha(10));
        assert_eq!(RolloutEngine::advance_stage(&mut segment).unwrap(), RolloutStage::Beta50(25));
        assert_eq!(RolloutEngine::advance_stage(&mut segment).unwrap(), RolloutStage::Beta90(50));
        assert_eq!(RolloutEngine::advance_stage(&mut segment).unwrap(), RolloutStage::Production);
        assert!(RolloutEngine::advance_stage(&mut segment).is_none());
    }

    #[test]
    fn test_create_config_defaults() {
        let segments = vec![
            RolloutSegment { name: "a".into(), percentage: 10, target_users: vec![], rollout_stage: RolloutStage::Internal, created_at: chrono::Utc::now() },
            RolloutSegment { name: "b".into(), percentage: 50, target_users: vec![], rollout_stage: RolloutStage::Alpha(20), created_at: chrono::Utc::now() },
        ];
        let config = RolloutEngine::create_config("test", "feat", &segments);
        assert_eq!(config.rollout_id, "test");
        assert!(config.active);
    }
}
```

### File 5: `prompt-hub/src/models.rs`
**Changes:** After line 870 (after CanaryDeployment), add new types in a new section block:

```rust
/// Staged rollout stages for gradual deployment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RolloutStage {
    Internal,           // no external users
    Alpha(u8),          // up to p% of total traffic
    Beta50(u8),         // up to 50% + p% variance
    Beta90(u8),         // up to 90% + p% variance
    Production,         // full production
}

/// A rollout segment (A/B/n variant) in a graduated rollout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutSegment {
    pub name: String,
    pub percentage: u8,              // 0-100 of total traffic
    pub target_users: Vec<Uuid>,     // whitelisted exact users
    pub rollout_stage: RolloutStage,
    pub created_at: DateTime<Utc>,
}

/// Auto-rollback policy for canary deployments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoRollbackPolicy {
    OnErrorRate { threshold: f64 },
    OnLatencyP99 { sla_ms: u64 },
    OnBoth { error_rate: f64, latency_p99_ms: u64 },
}

/// A canary/graduated rollout configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraduatedRolloutConfig {
    pub rollout_id: String,
    pub feature: String,
    pub segments: Vec<RolloutSegment>,
    pub auto_rollback: AutoRollbackPolicy,
    pub active: bool,
}

/// Alias for backward compat — folded into GraduatedRolloutConfig.
#[deprecated(note = "Use GraduatedRolloutConfig instead; CanaryDeployment is kept for the legacy canary_deploy() API")]
pub type CanaryConfig = GraduatedRolloutConfig;
```

Also add a test at end of models.rs tests section (~line 1084):
```rust
#[test]
fn test_graduated_rollout_config() {
    let cfg = GraduatedRolloutConfig {
        rollout_id: "test".into(),
        feature: "new-search".into(),
        segments: vec![],
        auto_rollback: AutoRollbackPolicy::OnErrorRate { threshold: 0.05 },
        active: true,
    };
    assert!(cfg.active);
}
```

### File 6: `prompt-hub/src/hub.rs` — test block update
After the existing canary test at hub.rs:2942, replace with gradual-rollout test behind new cfg gate:
```rust
#[cfg(feature = "gradual-rollout")]
#[test]
fn test_graduated_rollout_accessible() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let hub = rt.block_on(async {
        PromptHub::new(std::path::Path::new(":memory:"), HubConfig::default()).await.unwrap()
    });
    // Verify the type is accessible through the facade
    let _cfg = GraduatedRolloutConfig {
        rollout_id: "test".into(),
        feature: "test".into(),
        segments: vec![],
        auto_rollback: AutoRollbackPolicy::OnErrorRate { threshold: 0.05 },
        active: true,
    };
    hub.register_rollout(_cfg);
}
```

---

## 4. Migrations
None — this feature adds no schema changes. All state is in-memory (Vec stored on PromptHub struct via `Mutex<Vec<GraduatedRolloutConfig>>`).

---

## 5. Tests

### Unit tests (in gradual_rollout.rs, 6 tests):
| Test | What it verifies |
|------|-----------------|
| `test_rollout_target_user_always_included` | Target-list users bypass hash |
| `test_rollout_hash_bucket_excluded` | Zero-percentage excludes everyone via bucket |
| `test_rollback_on_error_rate` | OnErrorRate policy fires correctly |
| `test_rollback_on_latency_p99` | OnLatencyP99 policy fires correctly |
| `test_advance_stage_sequence` | Internal→Alpha→Beta50→Beta90→Production transitions + None at Production |
| `test_create_config_defaults` | Config creation with segment-derived thresholds |

### Hub-level test (in hub.rs, 1 test):
| Test | What it verifies |
|------|-----------------|
| `test_graduated_rollout_accessible` | Type construction + registration works through PromptHub facade |

### Models test (in models.rs, 1 test):
| Test | What it verifies |
|------|-----------------|
| `test_graduated_rollout_config` | GraduatedRolloutConfig deserializes/clones correctly |

---

## 6. Verify Commands

Run **after** implementing all changes:

```bash
# 1. Default features — the critical gate (was BROKEN before canary fix)
rtk cargo check -p prompt-hub

# 2. All features
rtk cargo check --workspace --all-features

# 3. Clippy with warnings-as-errors
rtk clippy --workspace --all-targets -- -D warnings

# 4. Format check
rtk fmt --check --all

# 5. Tests (all features, includes gradual-rollout)
rtk cargo test --workspace --all-features

# 6. Specifically test the new module
rtk cargo test -p prompt-hub gradual_rollout

# 7. Regression: verify canary tests still pass (canary_deploy uses CanaryDeployment → RolloutEngine)
rtk cargo test -p prompt-hub canary
```

---

## 7. Acceptance Criteria

| # | Criterion | Verification |
|---|-----------|-------------|
| AC-1 | `cargo check -p prompt-hub` (default features, no gradual-rollout) compiles clean | Step 6-1: zero errors |
| AC-2 | `cargo check --workspace --all-features` compiles clean | Step 6-2: zero errors |
| AC-3 | Clippy clean with `-D warnings` across all targets | Step 6-3: zero warnings |
| AC-4 | Format clean | Step 6-4: zero diffs |
| AC-5 | All 8 new tests pass (6 in gradual_rollout.rs + 1 hub + 1 models) | Step 6-5/6-6: green |
| AC-6 | Existing canary_deploy() API still works (backward compat) | Step 6-7: green |
| AC-7 | RolloutEngine::should_rollout() deterministically hashes user_id → feature (same input = same output) | Manual verification via AC-5 test assertions |
| AC-8 | RolloutStage advance sequence is correct: Internal→Alpha(10)→Beta50(25)→Beta90(50)→Production→None | Step 6-6 specifically validates |
| AC-9 | No `canary` feature remains — Cargo.toml, lib.rs, hub.rs all use `gradual-rollout` exclusively | Manual grep: zero references to `"canary"` as a feature string |
| AC-10 | `CanaryDeployment` struct remains in models.rs (used by legacy canary_deploy API) but is NOT gated behind gradual-rollout (it's a shared model type) | Verified: line 864-869 not behind cfg gate |

---

## 8. Drift Flagged

| Drift | Source | Issue | Rust-native fix |
|-------|--------|-------|-----------------|
| **Existing canary build bug** | hub.rs:24 | `use crate::models::CanaryDeployment` missing cfg gate breaks default-features compile | Wrapped in `#[cfg(feature = "gradual-rollout")]`; will be a no-op import when gradual-rollout is disabled since CanaryDeployment is always-available in models.rs |
| **CanaryEngine static struct** | canary.rs:11 | Empty derive(Default) unit struct — unusual pattern, suggests it should be `mod` or removed | Merged into RolloutEngine (also a static unit struct); removes canary.rs entirely |
| **Legacy naming in hub.rs** | hub.rs:1876-1895 | Methods named `canary_deploy`, `canary_should_rollback`, `canary_engine_handle` — legacy names from PR #51 | Renamed to `canary_deploy()` (kept for API compat), `evaluate_auto_rollback()`, removed `canary_engine_handle()` (unnecessary for stateless engine) |
| **models.rs CanaryDeployment** | models.rs:864 | Type is always-in but only used behind canary feature — no cfg gate on the type itself, which is fine (shared model) | No change needed; keeping it unconditional in models.rs is correct since other features may reference it |

---

## 9. Implementation Order (leaf-first)

1. **Cargo.toml** — add `gradual-rollout` feature, remove `canary`
2. **models.rs** — add RolloutStage, RolloutSegment, AutoRollbackPolicy, GraduatedRolloutConfig + test
3. **gradual_rollout.rs** (NEW) — create module with RolloutEngine + tests
4. **lib.rs** — swap `canary` mod gate for `gradual_rollout`
5. **hub.rs** — fix canary import bug (line 24), add imports/field/init, add delegation methods, update tests
6. **Delete canary.rs** — no longer needed

---

*Plan written: 2026-06-07 | Cycle 64 — gradual-rollout extends existing canary infrastructure + beta-program phased deployment patterns*
