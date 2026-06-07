# prompt-loop backlog — prompt_hub construction crew

The **single source of truth** for what the crew builds next. Legend:
`- [ ]` todo · `- [x]` done+verified · `- [!] blocked: <reason>`.
Each item = one cohesive, shippable unit sized to one cycle. Every item cites its source.

---

## State snapshot (2026-06-07 DISCOVER)

- `cargo check --workspace --all-features`: **0 errors** (3 crates compiled)
- `cargo clippy --workspace --all-targets -- -D warnings`: **clean**
- `cargo test --workspace --all-features`: **694 passed, 1 ignored**
- `cargo doc --workspace --all-features --no-deps`: **0 warnings**
- CI (last 5 runs): all green (Qodana/CI/Security/Push)
- `gh issue list`: no open issues
- swarm PR #49 was a phantom merge — zero `.rs` files changed. Swarm module (878 lines, 19 tests) remains completely unwired.

---

## P0: Critical

_Nothing. All gates green — build, clippy, tests, docs, CI all pass._

---

## P1: Feature completion (largest unwired modules with real tested code → wire into PromptHub)

The 3 highest-value candidates are the largest unwired modules with the most tests and clear pub APIs. Each is a self-contained domain ready to be wired behind its own method on `PromptHub`.

- [ ] **Wire `swarm::SwarmRoleRegistry` (878 lines, 19 tests, 22 pub items including `validate_swarm_roles`, `generate_full_handoff_chain`) into PromptHub — add `manage_swarm()` accessor + validation/bundle delegation methods**
  — source: `prompt-hub/src/swarm.rs`; PR #49 claimed wiring but was phantom merge (zero .rs changes); provenance: self-discovery

- [x] **Wire `pollination` module (410 lines, 10 tests) into PromptHub — add pollination-related Hub methods (+3 tests)** (#53 → merged ✅)
  — source: `prompt-hub/src/pollination.rs`; large tested module with clear public interface awaiting routing; provenance: self-discovery

- [x] **Wire `satisfaction::SatisfactionCollector` (374 lines, 14 tests) into PromptHub — add CSAT/NPS recording + metrics methods (+6 tests)** (#54 → merged ✅)
  — source: `prompt-hub/src/satisfaction.rs`; module has real tested logic for post-op satisfaction; provenance: self-discovery

---

## P2: Feature flag hygiene (~30 dead features → audit, gate, or remove)

Many feature declarations have zero `cfg(feature = "...")` gates on any source file. Some correspond to modules that exist (and are wired into hub.rs), others are orphaned declarations with no module at all.

- [ ] **Audit the 30+ dead feature flags in `prompt-hub/Cargo.toml` — for each, decide: wire behind a real cfg gate, remove from features table, or keep as stub for planned work**
  — Modules with NO corresponding feature: `audit`, `auth`, `config`, `defaults`, `error`, `hooks`, `hub`, `lib`, `lock`, `metrics`, `models`, `search`, `storage`, `sync`, `templates`, `tokens` (all are always-on, which is fine)
  — Features with module but no cfg gate: `vibe`(wired via hub.vibe_code()), `multimodal`(309 lines/21 tests), `preview`(477/1), `privacy`(wired via hub.scan_privacy()), `quality`/`quality_gate`, `rollback`(wired via hub.rollback()), `cost`(wired via hub.estimate_cost()), `confidence`(wired via hub.score_confidence()), `learn`(wired via hub.learn_from_feedback()), `fallback`(wired via hub.fallback_chain())
  — Features with NO module at all (true dead): `accessibility`, `auto-purge`, `beta-program`, `chaos`, `chaos-automation`, `cost-limits`, `ffi`, `gather`, `gradual-rollout`, `load-balance`, `local-llm`, `malware-scan`, `mobile`, `multi-provider`, `offline`, `qdrant`, `sandbox`, `touch`, `voice`, `voice-anonymize`
  — source: `prompt-hub/Cargo.toml` feature table vs. `grep -rl 'cfg(feature' prompt-hub/src/`; provenance: self-discovery

---

## P3: Quality & documentation (from TODO.md V section)

- [ ] **Regenerate `docs/audits/qodana.sarif.json`** — committed SARIF (2026-06-04) is stale; 87 findings need fresh triage
  — source: TODO.md line 26, V section; blocked on QODANA_TOKEN + Docker (external tooling wall per loop_state); provenance: TODO.md

- [ ] **Complete API documentation for all Hub methods** (`prompt-hub/src/hub.rs`) — add doc comments with examples
  — source: TODO.md P4 section; provenance: TODO.md

- [ ] **Document feature flags table in README.md** — map each flag to module and use case
  — source: TODO.md P4 section; provenance: TODO.md

- [ ] **Add crate-level docs in lib.rs** (`//!` doc comment with quickstart example)
  — source: TODO.md P4 section; provenance: TODO.md

---

## P4: Configuration hardening (from TODO.md V section — pre-existing)

- [!] **Default identity lacks `Write` capability for non-operator callers**
  — The CLI was fixed by PR #41 (`cli_identity()` returns `local_operator` with Read+Write+Admin), but the server's `default_agent()` and any direct `PromptHub::new()` without explicit config still produce `anonymous` (Read-only) identity. Any programmatic user who constructs a `PromptHub` directly gets write-denied mutations.
  — source: TODO.md line 20–24, V section; `prompthub-server/src/routes.rs` line ~64 (`default_agent()`); provenance: TODO.md + code inspection

---

## What was built across sessions (merged to main)

### SMART_EMBEDDING EPIC (PRs #44/#45/#46/#47/#48 — complete)
- Extract pluggable Embedder trait + HashEmbedder backend (+7 tests)
- Write prompt embeddings on index via Embedder (storage helpers + integration test)
- Select embedder backend from HubConfig (e2e verified)
- Wire ort-based OrtEmbedder behind smart-ort feature + HubConfig selection
- Real ONNX inference: lazy model download, tokenizers, ort::Session, [CLS] extraction, L2-normalize

### Feature wiring (PRs #49–#51 — verify status)
- [x] Wire `quality_gate::QualityGate` (467 lines) → hub.rs `run_quality_gate()` + field (PR #50, real 138-line .rs diff)
- [x] Wire `lineage::LineageTracker` (439 lines) → hub.rs delegation methods + field (PR #51, real 227-line .rs diff)
- [ ] Wire `swarm::SwarmRoleRegistry` (878 lines) — **PHANTOM**: PR #49 had zero .rs changes; module exists but is NOT wired

### Initial setup cycles (PRs #27/#30–#48)
- sha2 0.11 build fix
- Qodana triage (remove 32 unused deps + build fix)
- Prometheus text exposition via otel
- `prompthub metrics` CLI subcommand
- CLI tracing logs → stderr
- RUSTDOCFLAGS=-D warnings in CI
- Docker/Dockerfile verify + .cliff.toml
- CLI local operator identity (RBAC) — PR #41, `cli_identity()` → `AgentIdentity::local_operator()`
- Bench compile fix (criterion::black_box)

---

*Last update: 2026-06-07T00:00:00Z by DISCOVER. Fresh discovery recommended every cycle.*
