# prompt-loop backlog — prompt_hub construction crew

The **single source of truth** for what the crew builds next. Legend:
`- [ ]` todo · `- [x]` done+verified · `- [!] blocked: <reason>`.
Each item = one cohesive, shippable unit sized to one cycle. Every item cites its source.

---

## State snapshot (2026-06-07 DISCOVER)

- `cargo check --workspace --all-features`: **GREEN** (3 crates compiled)
- `cargo clippy --workspace --all-targets -- -D warnings`: **clean**
- `cargo test --workspace --all-features`: **707 passed, 1 ignored** (10 suites)
- `cargo doc --workspace --all-features --no-deps`: **0 warnings**, 3 docs generated
- CI (last 5 runs): all green (CI/Security/Qodana)
- `gh issue list`: no open issues
- swarm PR #49 phantom merge was fixed by s8 PR #52 — swarm.rs is wired (manage_swarm at hub.rs:729)

---

## P0: Critical

_Nothing. All gates green — build, clippy, tests, docs, CI all pass._

---

## P1: Feature completion (largest unwired modules with real tested code → wire into PromptHub)

All previously open P1 items have been wired and merged (PRs #52-#54). The swarm phantom issue from prior DISCOVER was confirmed resolved by s8 PR #52. No remaining P1 candidates of similar size/quality exist in the current tree.

- [x] **Wire `swarm::SwarmRoleRegistry` (878 lines, 19 tests) into PromptHub** — PR #52 merged ✅
- [x] **Wire `pollination::CrossAgentPollination` (410 lines, 10 tests)** — PR #53 merged ✅
- [x] **Wire `satisfaction::SatisfactionTracker` (374 lines, 14 tests)** — PR #54 merged ✅

---

## P2: Feature flag hygiene (~49 features in Cargo.toml)

Audit discovered the full picture. Of 49 feature declarations:

- **8 wired via cfg** (used in source): `handlebars`, `otel`, `plugins`, `smart-ort`, `tera`, `tiktoken` + smart+tls (partial, dep-only)
- **9 stub features** (feature=[]. module exists, hub.rs wiring exists, no cfg gate): `vibe`(26 pub), `privacy`(10 pub), `rollback`(16 pub), `cost`(6 pub), `confidence`(11 pub), `learn`(13 pub), `fallback`(10 pub), `multimodal`(13 pub), `satisfaction`(29 pub) — all wired into hub.rs as always-on, feature flag is a no-op
- **~32 dead features** (no module, no source refs): `beta-program`, `chaos-automation`, `cost-limits`, `gradual-rollout`, `malware-scan`, `multi-provider`, `offline`, `qdrant`, `sandbox`, `voice-anonymize`, `load-balance`, `local-llm`, and others with zero existence in codebase
- **~9 features with module but unclear purpose** (feature declared, no cfg gate): `quota`(module exists), `provider-health`(→ provider_health mod), `accessibility`(4 refs in code, no gate), `ffi`(6 refs, no gate), `quality`(10 refs, no gate), `touch`(1 ref), `voice`(2 refs), `chaos`(canary module, 3 pub items)

Proposed shippable unit: **Remove dead features + convert stub features to real cfg gates** — one coherent refactor of Cargo.toml with corresponding module-level gating in lib.rs/hub.rs. Splitting this into multiple cycles is possible but defeats the purpose; one item covering all three categories (dead/unused, stub→real, orphan modules) is a single Cargo.toml + import refactoring session.

- [ ] **Audit and clean up 49 feature flags in `prompt-hub/Cargo.toml`: remove dead features, convert stub features to real cfg gates or delete the feature key entirely**
  — source: direct grep of Cargo.toml `[features]` section vs `grep -rn 'cfg(feature' prompt-hub/src/` + module pub-count cross-reference; provenance: self-discovery
  — scope: remove ~32 dead features, gate 9 stub features behind real `#[cfg]`, or delete feature keys from modules that should be always-on

---

## P3: Quality & documentation (from TODO.md V section)

- [ ] **Regenerate `docs/audits/qodana.sarif.json`** — committed SARIF (1.4MB, 31961 lines) is stale; fresh CI run exists but output not re-committed
  — source: TODO.md line 26, V section + CI shows Qodana completed green; blocked on QODANA_TOKEN + Docker locally; provenance: TODO.md
- [ ] **Complete API documentation for all Hub methods** (`prompt-hub/src/hub.rs`) — add doc comments with examples
  — source: TODO.md P4 section; hub.rs has ~20+ pub methods, most lack `///` docs; provenance: TODO.md + code inspection
- [ ] **Document feature flags table in README.md** — map each flag to module and use case
  — source: README.md currently only documents `tui`; all other flags undocumented for end users; provenance: README.md inspection
- [ ] **Add crate-level docs in lib.rs** (`//!` doc comment with quickstart example)
  — source: lib.rs line 8 uses `#![doc = include_str!("../README.md")]` as crate docs — functional but lacks a Rust-specific quickstart example; provenance: TODO.md P4 section

---

## P4: Configuration hardening (from TODO.md V section — pre-existing)

- [!] **Default identity lacks `Write` capability for non-operator callers**
  — `AgentIdentity::default()` in `prompt-hub/src/models.rs:139` returns `anonymous` with empty capabilities. The server's `default_agent()` in `prompthub-server/src/routes.rs:60` correctly grants Read+Write, so the HTTP API is fine. The P4 concern only affects programmatic `PromptHub::new()` without explicit config or direct callers using `AgentIdentity::default()`. The CLI was fixed via PR #41 (`cli_identity()` → `local_operator`). This is a real edge case but not a build-blocker and has a documented workaround (`AgentIdentity::local_operator()`).
  — source: TODO.md line 20-24, V section + `prompt-hub/src/models.rs:139` + `prompthub-server/src/routes.rs:60`; provenance: TODO.md + code inspection

---

## What was built across sessions (merged to main)

### SMART_EMBEDDING EPIC (PRs #44/#45/#46/#47/#48 — complete)
- Extract pluggable Embedder trait + HashEmbedder backend (+7 tests)
- Write prompt embeddings on index via Embedder (storage helpers + integration test)
- Select embedder backend from HubConfig (e2e verified)
- Wire ort-based OrtEmbedder behind smart-ort feature + HubConfig selection
- Real ONNX inference: lazy model download, tokenizers, ort::Session, [CLS] extraction, L2-normalize

### Feature wiring (PRs #50–#54 — all merged)
- [x] Wire `quality_gate::QualityGate` → hub.rs `run_quality_gate()` (PR #50)
- [x] Wire `lineage::LineageTracker` → hub.rs delegation methods (PR #51)
- [x] Wire `swarm::SwarmRoleRegistry` → hub.rs `manage_swarm()` + validation/bundle (PR #52)
- [x] Wire `pollination::CrossAgentPollination` → extract_pollination_patterns() + mutex access (PR #53)
- [x] Wire `satisfaction::SatisfactionTracker` → CSAT/NPS recording + metrics (PR #54)

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

With P1 exhausted (all wired/merged), P4 is edge-case-only, and P3 items are documentation-heavy with no code gates, the most impactful next item is **P2: feature flag hygiene**. It is a single coherent refactoring (Cargo.toml edit + lib.rs gating adjustments) that would significantly improve the project's signal-to-noise ratio. After P2, the remaining P3 items are trivial in effort (copy-editing).

*Last update: 2026-06-07T18:45:00Z by DISCOVER. Fresh discovery recommended every cycle.*
