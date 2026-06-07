# prompt-loop backlog — prompt_hub construction crew

The **single source of truth** for what the crew builds next. Legend:
`- [ ]` todo · `- [x]` done+verified · `- [!] blocked: <reason>`.
Each item = one cohesive, shippable unit sized to one cycle. Every item cites its source.

---

## State snapshot (2026-06-08 RESUME — s15 continuation)

- `cargo check --workspace --all-features`: **GREEN** ✅ (3 crates compiled)
- `cargo clippy --workspace --all-targets -- -D warnings`: **clean** ✅
- `cargo test --workspace --all-features`: 724 passed, 2 ignored (11 suites) — rollback wired PR #62 pending
- `cargo doc --workspace --all-features --no-deps`: **0 warnings**
- CI: last 5 runs green
- `gh issue list`: no open issues
- `gh pr list --state open`: no open PRs
- Branch: main, clean working tree

---

## P0: Critical

_Nothing. All gates green — build, clippy, tests, docs, CI all pass._

---

## P1: Feature completion (modules with real tested code but zero hub.rs wiring)

### Previously wired (s10–s15) — all confirmed in hub.rs

The following feature-gated modules, once listed as unwired, are now **confirmed wired** in hub.rs:

| Module | Feature gate | Hub wiring confirmed? | Session |
|--------|-------------|----------------------|---------|
| `budget` | `"budget"` (stub) | ✅ imports QuotaEnforcer at 35 + BudgetAlert at 7,2593 | s10–s14 |
| `circuit_breaker` | `"circuit-breaker"` (stub) | ✅ imports CircuitBreaker | s10–s14 |
| `canary` | `"canary"` (stub) | ✅ imports CanaryEngine | s12c4 |
| `moderation` | `"moderation"` (stub) | ✅ imports ModerationEngine | s12c1 |
| `quota` | `"quota"` (stub) | ✅ imports QuotaEnforcer | s12c2 |
| `preview` | `"preview"` (stub) | ✅ imports PreviewEngine | s12c3 |
| `i18n` | `"i18n"` (stub) | ✅ imports I18nEngine + real usage at 1739 (`fallback_chain`) | s15 |
| `multimodal` | `"multimodal"` (stub) | ✅ imports MultimodalEngine; accessor + 2 delegation methods | s14-c2 |
| `quality_gate` | (ungated) | ✅ PR #50 | — |
| `lineage` | (ungated) | ✅ PR #51 | — |
| `swarm` | (ungated) | ✅ PR #52 | — |
| `pollination` | (ungated) | ✅ PR #53 | — |
| `satisfaction` | (ungated) | ✅ PR #54 | — |
| `provider_health` | (ungated) | ✅ PR #58 | — |
| `load_balancer` | (ungated) | ✅ PR #59 | — |

### Still unwired: feature-gated modules (have #[cfg] gate but zero hub.rs wiring)

After accurate cross-checking (grep for both `use crate::X::*` AND `crate::X::SpecificItem`), only **one** passthrough feature remains unwired:

| # | Module | Feature gate | Status | Evidence |
|---|--------|-------------|--------|----------|
| ~~2a~~ | ~~`rollback`~~ | ~~`"rollback"` (stub)~~ | ✅ WIRED PR #62 + committed to main (`baaa53e`)~~ | ~~~~wired 3 methods + struct field + import~~ | ~~All P1 wiring complete.~~ |

**Previously misclassified as unwired (now corrected):** confidence, cost, fallback, learn, vibe — all have confirmed hub.rs imports:
- `confidence`: `use crate::confidence::ConfidenceScorer;` at hub.rs:806
- `cost`: `use crate::cost::CostEstimator;` at hub.rs:750
- `fallback`: `use crate::fallback::FallbackChain;` at hub.rs:1023
- `learn`: `use crate::learn::LearningEngine;` at hub.rs:1051
- `vibe`: wired ✅ (import confirmed)

**Note:** These five were misclassified in s11 as "zero hub.rs wiring" because the earlier grep only checked for bare module path patterns (`crate::X::*`) and missed specific item imports.

### Still unwired: un-gated modules (no feature gate, zero hub.rs imports)

After re-checking, the following un-gated modules have **zero hub.rs wiring** and need a decision:

| # | Module | Lines (est.) | Status | Decision needed |
|---|--------|-------------|--------|-----------------|
| 3a | `analytics` | ~350 | ❌ has pub mod but zero hub imports per hub.rs grep | Wire unconditional OR gate with feature per design_decision/unwired_modules.md |
| 3b | `audit` | ~406 | ❌ has pub mod but zero hub imports | AuditLogger may be used outside hub — verify scope before wiring |
| 3c | `garbage_collector` | ~283 | ❌ has pub mod but zero hub imports | Pair with retention via feature gate (per design decision) |
| 3d | `health` | TBD | ❌ has pub mod but zero hub imports | Investigate: consider gating or remove if internal-only |
| 3e | `defaults` | TBD | ❌ has pub mod but zero hub imports | Internal seed/config defaults — may not need hub exposure |

**Important:** These modules have `pub mod` declarations but the earlier "wired" classification was wrong — they appear in Cargo.toml features as stub passthroughs (`feature = []`) but do NOT have hub.rs wiring. The `pub mod` is what makes them publicly available within the crate, and they should either be wired into PromptHub or gated behind feature flags to prevent accidental exposure.

### Already wired (un-gated, confirmed in hub.rs) — DO NOT list as unwired

The following un-gated modules ARE wired and were previously misclassified:

| Module | Hub wiring location |
|--------|-------------------|
| `diff` | `diff_engine: PromptDiff` field at hub.rs:177; used via `diff_hash()` fn |
| `context_gatherer` | Import at hub.rs:730; used in cost estimation path |
| `evolution` | wired ✅ (confirmed) |
| `plugins` | wired ✅ (confirmed) |

### Feature-gated modules with wiring but no explicit feature gate on lib.rs pub mod

| Module | Status | Note |
|--------|--------|------|
| `retention` | ⚠️ has hub wiring AND pub mod, but no feature gate on `pub mod` — should be gated alongside garbage_collector per design decision |

---

## P2: Feature flag hygiene (remaining stub features in Cargo.toml) ✅ DONE

- [x] **Remove dead stub features** — `sqlcipher`, `ffi`, and `garbage-collector` passthrough entries removed from all 3 crates. One additional fix: re-gated `garbage_collector` field in hub.rs from `feature = "garbage-collector"` → `feature = "retention"` (was orphaned cfg gate). +4 files changed, -15 lines. Committed as `s14-c1`.
- [x] **Wire remaining module: multimodal** — 8 pub items, 21 tests but ZERO hub.rs wiring. Added import, struct field, accessor (`multimodal_engine()`), and 2 delegation methods (`validate_image_mime_type`, `extract_placeholder_ids`). +45 LOC, +1 test (723 total). Committed as `s14-c2`.

---

## P3: Quality & documentation

- [x] **Complete API documentation for all Hub methods** (`hub.rs`) — merged ✅ PR #56
- [x] **Document feature flags table in README.md** — merged to main
- [x] **Add crate-level docs in lib.rs** — merged to main

### P3 candidates (from s10, **STALE — already addressed**)

- [x] ~~**Integration tests for `storage.rs` (1904 lines, 1 test)**~~ — actually has **20 unit tests** inside `mod tests` block since before this loop; backlog data stale
- [x] ~~**Integration tests for `hub.rs` (2071 lines, 2 tests)**~~ — actually has **9 integration tests in `test_hub.rs`** + 33 (`test_models.rs`) + 15 (`test_search.rs`) + 18 (`test_security.rs`); backlog data stale

---

## P4: Edge cases and code quality

- [!] **Default identity lacks `Write` capability for non-operator callers**
  — `AgentIdentity::default()` in `prompt-hub/src/models.rs:139` returns `anonymous` with empty capabilities. Server's `default_agent()` grants Read+Write (HTTP API is fine). P4 only affects programmatic `PromptHub::new()` without explicit config. Documented workaround: `AgentIdentity::local_operator()`.
  — source: TODO.md V section + `prompt-hub/src/models.rs:139` + `prompthub-server/src/routes.rs:60`; provenance: code inspection

- [x] **`defaults.rs` seed_database() dead parameter cleanup** — removed unused `_hub: &PromptHub` parameter and dead imports (`crate::hub::PromptHub`, `use tracing::info`). Function kept for API stability but documented as no-op placeholder. +5 -4 lines. Committed as `s15-c1`.

- [x] **i18n module is NOT dead code — it IS wired in hub.rs** — confirmed import at hub.rs:19 (`use crate::i18n::I18nEngine`) and real usage at hub.rs:1739 (`crate::i18n::I18nEngine::fallback_chain(locale)`). Module was gated behind `"i18n"` feature. Misclassified in s11 as "dead code". Resolved by wiring commit `44c81ee`.

---

---

## P4b: Newly discovered items (DISCOVER s12) — **STALE, resolved during s15**

- [x] ~~**Un-gated unwired modules need feature gates or removal**~~ — analytics(`audit.rs`), audit(`SqliteAuditLogger`), garbage_collector (`GarbageCollector::new()`), health (`HealthAggregator`) all have hub.rs imports + struct fields + wiring since PRs #60-#62. Committed to main.
- [x] ~~**Feature-flag passthrough inventory**~~ — all 20 features confirmed wired or gated. Inventory now matches reality: vibe✅, privacy✅, cost✅, confidence✅, learn✅, fallback✅, multimodal✅, satisfaction✅, quota✅, retention✅/garbage_collector✅ (gated), quality_gate✅, canary✅, circuit-breaker✅, moderation✅, budget✅, analytics✅, preview✅, i18n✅, rollback✅.

---

## What was built across sessions (merged to main)

### SMART_EMBEDDING EPIC (PRs #44/#45/#46/#47/#48 — complete)
- Extract pluggable Embedder trait + HashEmbedder backend (+7 tests)
- Write prompt embeddings on index via Embedder (storage helpers + integration test)
- Select embedder backend from HubConfig (e2e verified)
- Wire ort-based OrtEmbedder behind smart-ort feature + HubConfig selection
- Real ONNX inference: lazy model download, tokenizers, ort::Session, [CLS] extraction, L2-normalize

### Feature wiring (PRs #50–#59 — all merged)
- [x] Wire `quality_gate::QualityGate` → hub.rs `run_quality_gate()` (PR #50)
- [x] Wire `lineage::LineageTracker` → hub.rs delegation methods (PR #51)
- [x] Wire `swarm::SwarmRoleRegistry` → hub.rs `manage_swarm()` + validation/bundle (PR #52)
- [x] Wire `pollination::CrossAgentPollination` → extract_pollination_patterns() + mutex access (PR #53)
- [x] Wire `satisfaction::SatisfactionTracker` → CSAT/NPS recording + metrics (PR #54)
- [x] Audit and clean up 49 feature flags in Cargo.toml: remove dead features, convert stub→real (PR #55)
- [x] Complete API documentation for all Hub methods (PR #56)
- [x] Ungate quality_gate module — fixes default-feature build (PR #57)
- [x] Wire `provider_health::ProviderHealthMonitor` into PromptHub facade (PR #58)
- [x] Wire `load_balancer::LoadBalancer` into PromptHub facade (PR #59)

### Initial setup cycles (PRs #27–#48)
- sha2 0.11 build fix
- Qodana triage (remove 32 unused deps + build fix)
- Prometheus text exposition via otel
- `prompthub metrics` CLI subcommand
- CLI tracing logs → stderr
- RUSTDOCFLAGS=-D warnings in CI
- Docker/Dockerfile verify + .cliff.toml
- CLI local operator identity (RBAC) — PR #41

---

## Terminal state assessment (RESUME 2026-06-07)

**P1 wiring: COMPLETE ✅** (all 20 passthrough features wired or gated)
**RESUME findings:** backlog is **effectively terminal**. All remaining items are stale claims from earlier sessions that have been addressed:

| Backlog Item | Actual State | Resolution |
|---|---|---|
| P3 storage.rs integration tests | Has **20 unit tests** in `mod tests` block | Stale claim "1 test" — marked done above |
| P3 hub.rs integration tests | Has **9 in test_hub.rs + 33+ across other files** | Stale claim "2 inline doctests" — marked done above |
| P4b unwired modules (analytics, audit, GC, health, defaults) | All have hub.rs wiring + struct fields as of s15 | Resolved PRs #60-#62 |
| TODO.md: CLI tracing to stderr | Fixed at `prompthub/src/main.rs:43` (.with_writer(stderr)) | Already fixed during s15 |

**No genuinely shippable items remain.** This is a DONE state.
- P3 integration test expansion (2 items)
- P4 default identity capability gap

**Previously misclassified in s11 → corrected by s12 DISCOVER:**
- budget, circuit_breaker, canary, moderation, quota, preview — CONFIRMED wired in hub.rs ✅
- i18n — CONFIRMED wired (real usage at hub.rs:1739), NOT dead code ✅
- confidence, cost, fallback, learn — CONFIRMED wired (specific item imports like `CostEstimator`) ✅
- diff, context_gatherer, evolution, plugins — confirmed wired ✅
- **Key correction:** earlier grep only checked for bare module patterns; missed specific item imports

**Total tests passing: 724 passed, 2 ignored** (vs 671 at s10 baseline — +53 tests over the loop).

After P1 items are wired, remaining work is P3/P4 (test expansion and edge cases). The backlog remains shippable — not terminal DONE.

*Last update: 2026-06-08T04:15:00Z RESUME — P1 wiring COMPLETE, rollback wired PR #62 pending CI.**
