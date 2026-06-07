# prompt-loop backlog — prompt_hub construction crew

The **single source of truth** for what the crew builds next. Legend:
`- [ ]` todo · `- [x]` done+verified · `- [!] blocked: <reason>`.
Each item = one cohesive, shippable unit sized to one cycle. Every item cites its source.

---

## State snapshot (2026-06-07 DISCOVER — session 11)

- `cargo check --workspace --all-features`: **GREEN** ✅ (3 crates compiled)
- `cargo clippy --workspace --all-targets -- -D warnings`: **clean** ✅
- `cargo test --workspace --all-features`: **710 passed, 2 ignored** (11 suites) — +3 tests since s10
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

### 1a–1f: Wire remaining feature-gated modules into PromptHub façade

Six modules have `#[cfg(feature = "...")]` gates in lib.rs and **zero** hub.rs wiring — they compile behind feature flags but are unreachable from the user-facing PromptHub type. Each has real, well-tested implementations with pub API.

| # | Module | Lines | Pub items | Tests | Cargo.toml feature |
|---|--------|-------|-----------|-------|---------------------|
| 1a | `budget.rs` | 7.5K | 12 | 11 | `"budget"` (gated) |
| 1b | `circuit_breaker.rs` | 7.9K | 6 | 9 | `"circuit-breaker"` (gated) |
| ~~1c~~ | ~~`moderation.rs`~~ | ~~9.2K~~ | ~~9~~ | ~~10~~ | ~~`"moderation"` (gated)~~ — wired ✅ s12c1 |
| ~~1d~~ | ~~`quota.rs`~~ | ~~8.6K~~ | ~~10~~ | ~~10~~ | ~~`"quota"` (stub)~~ — wired ✅ s12c2 |
| ~~1e~~ | ~~`preview.rs`~~ | ~~15.9K~~ | ~~4~~ | ~~7~~ | ~~`"preview"` (gated)~~ — wired ✅ s12c3 |
| ~~1f~~ | ~~`canary.rs`~~ | ~~3.0K~~ | ~~4~~ | ~~6~~ | ~~`"canary"` (gated)~~ — wired ✅ s12c4 |

**Status update from s10:** LoadBalancer wired ✅ via PR #59, ProviderHealthMonitor wired ✅ via PR #58, satisfaction kept always-in (kept as PromptHub field). These six remain unwired.

**Each is a shippable unit:** Add `#[cfg(feature = "...")] use crate::X::*;` in hub.rs, struct field on `PromptHub`, and 1–3 delegation methods. Estimate: ~5–10 additions per module. One cycle each. Prioritize by impact (budget > circuit-breaker > moderation > quota > preview > canary).

**Provenance:** Direct grep of hub.rs for `crate::X` imports across all six modules returned 0 matches. Source pointers: `prompt-hub/src/budget.rs`, `circuit_breaker.rs`, `moderation.rs`, `quota.rs`, `preview.rs`, `canary.rs`.

### 1g–1k: Wire un-gated but unwired modules into PromptHub façade

These five modules have **no feature gate** on their `pub mod` in lib.rs and **zero hub.rs wiring**. They compile unconditionally but are unreachable from the PromptHub facade:

| # | Module | Lines | Pub items | Tests | Cargo.toml feature |
|---|--------|-------|-----------|-------|---------------------|
| 1g | `analytics.rs` | 352 | 15 | 11 | `"analytics"` (stub) |
| 1h | `audit.rs` | 406 | 7 | 14 | **none** (bare module, no feature entry) |
| 1i | `diff.rs` | 338 | 9 | 11 | **none** (bare module, no feature entry) |
| 1j | `garbage_collector.rs` | 283 | 13 | 11 | `"garbage-collector"` (stub passthrough — but lib.rs has NO cfg gate) |
| 1k | `retention.rs` | 290 | 15 | 11 | `"retention"` (stub) |

**Note:** These have a different shape from 1a–1f because they lack both feature gates and hub wiring. For these, consider whether to add proper feature gates in lib.rs before wiring into hub.rs, or wire them as unconditional features. Either way: each is shippable in one cycle.

---

## P2: Feature flag hygiene (remaining stub features in Cargo.toml) ✅ DONE

- [x] **Remove dead stub features** — `sqlcipher`, `ffi`, and `garbage-collector` passthrough entries removed from all 3 crates. One additional fix: re-gated `garbage_collector` field in hub.rs from `feature = "garbage-collector"` → `feature = "retention"` (was orphaned cfg gate). +4 files changed, -15 lines. Committed as `s14-c1`.

---

## P3: Quality & documentation

- [x] **Complete API documentation for all Hub methods** (`hub.rs`) — merged ✅ PR #56
- [x] **Document feature flags table in README.md** — merged to main
- [x] **Add crate-level docs in lib.rs** — merged to main

### P3 candidates (from s10, still valid)

- [ ] **Add integration tests for `storage.rs` (1904 lines, 1 test)** — largest single file with worst coverage ratio; one coherent integration test file covering create/get/list/update/delete/pagination
- [ ] **Add integration tests for `hub.rs` (2071 lines, 2 tests)** — PromptHub's 43 pub methods have only 2 inline doctests; needs a dedicated integration test suite

---

## P4: Edge cases and code quality

- [!] **Default identity lacks `Write` capability for non-operator callers**
  — `AgentIdentity::default()` in `prompt-hub/src/models.rs:139` returns `anonymous` with empty capabilities. Server's `default_agent()` grants Read+Write (HTTP API is fine). P4 only affects programmatic `PromptHub::new()` without explicit config. Documented workaround: `AgentIdentity::local_operator()`.
  — source: TODO.md V section + `prompt-hub/src/models.rs:139` + `prompthub-server/src/routes.rs:60`; provenance: code inspection

- [ ] **`defaults.rs` seed_database() has empty body with `_hub` dead parameter** (`prompt-hub/src/defaults.rs`) — never wired into any init flow; zero callers. Either implement seeding or remove.
  — source: `defaults.rs` line ~30; provenance: code inspection

- [ ] **i18n module is dead code from hub's perspective** (322 lines, 10 pub items, 12 tests) — zero callers in hub.rs or CLI. Complete module with no integration path. Consider removing or wiring.
  — source: `i18n.rs`; provenance: code inspection

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

## Terminal state assessment

**The backlog is NOT empty.** There are 11 actionable P1 items remaining:
- 6 feature-gated modules awaiting hub.rs wiring: budget, circuit-breaker, moderation, quota, preview, canary
- 5 un-gated-but-unwired modules awaiting decision and wiring: analytics, audit, diff, garbage_collector, retention

All six feature-gated modules (1a–1f) are the **highest priority** because their feature gates already exist in lib.rs — wiring them into hub.rs completes a full lifecycle with minimal refactoring. The un-gated group (1g–1k) needs a design decision on feature gating first.

After P1 items are wired, remaining work is P2 (stub feature cleanup) and P3/P4 (test expansion and edge cases). The backlog remains shippable — not terminal DONE.

*Last update: 2026-06-07T20:15:00Z by DISCOVER (s11). Fresh discovery recommended every cycle.*
