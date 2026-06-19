# Wire budget module into PromptHub facade — Plan

**Item:** P1a from s11 backlog (loop_state.md line 20)
**Module:** `prompt-hub/src/budget.rs` (~7.5K lines, 11 tests, 6 pub types/fns on `BudgetTracker`)
**Feature flag:** `budget = []` — already declared in `Cargo.toml:58`, no optional deps needed
**Blast radius:** **LOW** — zero existing callers of budget types (confirmed grep + kb). This is a greenfield wiring only.

---

## 1. Blast Radius

| Symbol | Existing callers in hub.rs / lib.rs | Risk |
|--------|--------------------------------------|------|
| `BudgetTracker` | 0 | Low |
| `BudgetConfig`  | 0 | Low |
| `BudgetAlert`   | 0 | Low |
| `budget` module | declared at `lib.rs:26` (un-gated) but never used in hub.rs | Low |

**Conclusion:** Safe to add wiring. No existing code touches the budget module — it is a clean insert.

---

## 2. Rust-Native Design Decisions

### Feature gating
- `budget` feature already exists in `Cargo.toml:58` as `budget = []`.
- All budget code in hub.rs must be gated with `#[cfg(feature = "budget")]`.
- The module declaration in `lib.rs` must also be gated (it is currently un-gated at line 26).

### Placement of logic
- Budget business logic stays entirely in `budget.rs` — this plan only wires it into the hub facade, no refactoring of `budget.rs`.
- The tracker is a simple thread-safe struct using `AtomicU64`/`AtomicBool` — **no async needed**. It will be `Arc<BudgetTracker>` on the hub struct (same pattern as `MetricsCollector`, `QualityGate`).

### Type exposure
- `BudgetTracker` (pub struct) — instantiated per-hub, stored as `Arc`
- `BudgetConfig` (pub, Serialize+Deserialize) — loaded from storage or config; no new persistence needed at wiring stage
- `BudgetAlert` (pub enum) — return type of `record_spend`

---

## 3. Files & Changes

### File 1: `prompt-hub/src/lib.rs` — gate the module declaration

**Change:** Add `#[cfg(feature = "budget")]` before the existing `pub mod budget;` line.

```
Line 26 (current):
    pub mod budget;

Replace with:
    #[cfg(feature = "budget")]
    pub mod budget;
```

### File 2: `prompt-hub/src/hub.rs` — four insertions

#### 2A. Import types (insert after line 18, before line 19)

Add a use import for the budget types (gated):

```rust
// Insert at line 18 (after `use crate::sync::{SyncEvent, SyncManager};`)
#[cfg(feature = "budget")]
use crate::budget::{BudgetAlert, BudgetConfig, BudgetTracker};
```

#### 2B. Add field on PromptHub struct (insert after line 134)

After the `load_balancer` field:

```rust
// Insert at line 135 (after `load_balancer: Arc<...LoadBalancer>,`)
    #[cfg(feature = "budget")]
    budget_tracker: Arc<BudgetTracker>,
```

#### 2C. Initialize in constructor `impl PromptHub::new()` (insert after line 193, inside the `Self { ... }` block)

Inside the struct initialization at lines 176-194, add after the `load_balancer:` line:

```rust
    // Insert at line 193 (after `load_balancer: Arc<...>,`)
    #[cfg(feature = "budget")]
    budget_tracker: Arc::new(BudgetTracker::default()),
```

Also add initialization **after** the hooks registration block (after line 198):

```rust
// Insert after line 198 (after `hub.hooks.register(Box::new(JunieHook));`)
    #[cfg(feature = "budget")]
    {
        info!("Budget tracker initialized with default $1000.00/month");
    }
```

#### 2D. Add delegation methods (insert after line 953, before the `// -- Quality gate` comment at line 955)

Add a section header and three methods:

```rust
// Insert at line 954 (before `// ── Quality gate`)

    // ── Budget tracking ────────────────────────────────────────────────

    /// Record a spend amount against the monthly budget.
    ///
    /// Increments the current spend counter and fires an alert if any
    /// configured threshold is crossed for the first time (50%, 80%, 100%).
    /// Requires the `budget` feature flag.
    ///
    /// # Arguments
    /// * `amount_usd` — Spend amount in US dollars to record.
    ///
    /// # Returns
    /// A [`BudgetAlert`] indicating if a threshold was crossed, or `None`.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn record_spend(&self, amount_usd: f64) -> BudgetAlert {
        let alert = self.budget_tracker.record_spend(amount_usd);
        if let BudgetAlert::None = alert {
            debug!("Recorded ${:.4} spend", amount_usd);
        }
        alert
    }

    /// Get the current monthly budget utilization as a percentage.
    ///
    /// Returns 0.0 if no budget is configured or if spend has not been reset
    /// for the billing period.
    /// Requires the `budget` feature flag.
    ///
    /// # Returns
    /// A float in the range [0.0, 100.0+] where >100.0 means over budget.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn budget_utilization(&self) -> f64 {
        self.budget_tracker.utilization_percent()
    }

    /// Get the current month's spend in USD.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn current_spend_usd(&self) -> f64 {
        self.budget_tracker.current_spend_usd()
    }

    /// Check whether the monthly budget has been exceeded.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn is_budget_exceeded(&self) -> bool {
        self.budget_tracker.is_exceeded()
    }

    /// Update the configured monthly budget amount.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn set_monthly_budget(&self, monthly_budget_usd: f64) {
        self.budget_tracker.set_budget(monthly_budget_usd);
    }

    /// Load a persisted [`BudgetConfig`] into the tracker.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn load_budget_config(&self, config: &BudgetConfig) -> Result<()> {
        self.budget_tracker.load_config(config)
    }

    /// Save the current budget state as a [`BudgetConfig`] for the given org.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn save_budget_config(&self, org_id: &str) -> Result<BudgetConfig> {
        self.budget_tracker.save_config(org_id)
    }

    /// Reset spend counters for a new billing period.
    #[cfg(feature = "budget")]
    #[instrument(skip(self))]
    pub fn reset_budget_period(&self) {
        self.budget_tracker.reset_period();
    }
```

---

## 4. Tests

No new tests needed for the wiring itself — the delegation methods are trivial pass-throughs (already covered by `budget.rs`'s 11 existing tests). However, add one hub-level integration test to verify the feature-gated path compiles and works end-to-end when the `budget` feature is enabled:

### File 3: `prompt-hub/src/hub.rs` — test at line ~2070 (before `mod tests` closing brace or appended to the existing `tests` module)

Insert inside the existing `#[cfg(test)] mod tests { ... }` block (after the existing tests, before the closing `}`):

```rust
// Insert near end of tests module (approximately line 2065)

    #[cfg(feature = "budget")]
    #[tokio::test]
    async fn test_budget_delegation() {
        use crate::budget::{BudgetAlert, BudgetConfig};
        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(dir.path(), test_config()).await.unwrap();

        // Default budget is $1000
        assert!(!hub.is_budget_exceeded());
        assert_eq!(hub.current_spend_usd(), 0.0);

        // Record spend and check utilization
        let alert = hub.record_spend(500.0);
        assert_eq!(alert, BudgetAlert::FiftyPercent);
        assert!((hub.budget_utilization() - 50.0).abs() < f64::EPSILON);

        // Exceed budget
        let alert = hub.record_spend(600.0);
        assert_eq!(alert, BudgetAlert::HundredPercent);
        assert!(hub.is_budget_exceeded());
        assert!((hub.budget_utilization() - 110.0).abs() < f64::EPSILON);

        // Save / load config round-trip
        let config = hub.save_budget_config("test-org").unwrap();
        assert_eq!(config.monthly_budget_usd, 1000.0);

        hub.reset_budget_period();
        assert_eq!(hub.current_spend_usd(), 0.0);
        assert!(!hub.is_budget_exceeded());
    }
```

---

## 5. Verify Commands

Run these **in order** after implementing:

```bash
# 1. Check default features (budget is NOT in default, so must test with budget feature on)
cd /home/drdave/Desktop/meta/prompt_hub && rtk cargo check --workspace

# 2. Check WITH the budget feature
cd /home/drdave/Desktop/meta/prompt_hub && rtk cargo check -p prompt-hub --features budget

# 3. Full workspace check (all features)
cd /home/drdave/Desktop/meta/prompt_hub && rtk cargo check --workspace --all-features

# 4. Clippy lint gate
cd /home/drdave/Desktop/meta/prompt_hub && rtk cargo clippy --workspace --all-targets --all-features -- -D warnings

# 5. Run the specific budget delegation test
cd /home/drdave/Desktop/meta/prompt_hub && rtk cargo test -p prompt-hub --features budget test_budget_delegation

# 6. Run ALL budget module tests
cd /home/drdave/Desktop/meta/prompt_hub && rtk cargo test -p prompt-hub --features budget -- budget::

# 7. fmt check
cd /home/drdave/Desktop/meta/prompt_hub && rtk cargo fmt --all -- --check

# 8. Run all workspace tests (regression)
cd /home/drdave/Desktop/meta/prompt_hub && rtk cargo test --workspace --all-features
```

---

## 6. Acceptance Criteria

| # | Criterion | Check |
|---|-----------|-------|
| AC1 | `cargo check --workspace` (default features, no `budget`) compiles green | Command 1 passes |
| AC2 | `cargo check -p prompt-hub --features budget` compiles green | Command 2 passes |
| AC3 | `cargo clippy --workspace --all-targets --all-features -- -D warnings` is clean | Command 4 passes |
| AC4 | `cargo fmt --all -- --check` reports no changes needed | Command 7 passes |
| AC5 | `test_budget_delegation` test (new, in hub.rs) passes | Command 5 passes |
| AC6 | All 11 existing budget module tests pass | Command 6 passes |
| AC7 | Full workspace test suite still green | Command 8 passes |
| AC8 | `lib.rs:26` module decl is gated with `#[cfg(feature = "budget")]` | Manual check |
| AC9 | All hub wiring fields/methods are gated with `#[cfg(feature = "budget")]` | Manual check |

---

## 7. Drift Flagged

None — this is a clean feature-gated module addition. No drifted instructions detected.

## 8. Insertion Order (leaf-first, no broken intermediate state)

1. `lib.rs:26` — gate the module decl (step 2A)
2. `hub.rs:18` — import line (step 2B)
3. `hub.rs:135` — struct field (step 2C)
4. `hub.rs:193` — constructor init (step 2D)
5. `hub.rs:198` — debug info block (step 2D)
6. `hub.rs:954` — delegation methods (step 2E)
7. `hub.rs` tests module — integration test (step "Tests")

---

**Risk assessment: LOW** — zero callers, clean feature-gated insertion following the same pattern used in PRs #50-#59 (cost, vibe, privacy, confidence, rollback, fallback, learn, quality, swarm, pollination, satisfaction, health_monitor, load_balancer).
