# prompt-loop backlog — prompt_hub construction crew

The **single source of truth** for what the crew builds next. Legend:
`- [ ]` todo · `- [x]` done+verified · `- [!] blocked: <reason>`.
Each item = one cohesive, shippable unit sized to one cycle. Every item cites its source.

---

## State snapshot (2026-06-07 DISCOVER)

- `cargo check --workspace --all-features`: **0 errors** (3 crates compiled)
- `cargo clippy --workspace --all-targets -- -D warnings`: **clean**
- `cargo test --workspace --all-features`: **685 passed, 1 ignored**
- `cargo doc --workspace --all-features --no-deps`: **0 warnings**
- CI (last 5 runs): all green (CI/Qodana/Security/Push)
- `gh issue list`: no open issues
- SMART_EMBEDDING epic shipped via PR #44–#48, all merged to main

---

## P0: Critical

_Nothing. All gates green — build, clippy, tests, docs, CI all pass._

---

## P1: Feature completion (scaffold → wire)

Three modules have 70+ lines of tested implementations with public APIs but are **never wired into `PromptHub`** and lack feature-gates in `Cargo.toml`. They are the highest-value candidates for "make them available at runtime."

- [x] **Wire `swarm::SwarmRoleRegistry` (878 lines, 22 pub items, 23 tests) into PromptHub — add a `manage_swarm()` method to hub.rs that creates/uses a SwarmRoleRegistry and validates role dependency graphs** (#49 → merged ✅)
  — source: `prompt-hub/src/swarm.rs` (largest unwired module; ADR-0008 vibe architecture references swarm coordination); provenance: self-discovery

- [x] **Wire `quality_gate::QualityGate` (443 lines, 11 pub items including Linter/SecurityScanner/PerformanceChecker traits) into PromptHub — add a `run_quality_gate()` method that invokes the gate pipeline** (#50 → merged ✅)
  — source: `prompt-hub/src/quality_gate.rs`; ADR-0008 mentions "QualityGate" as a pipeline stage; provenance: self-discovery

- [ ] **Wire `lineage::LineageTracker` (439 lines, 16 pub items, 15 tests) into PromptHub — add `get_prompt_lineage()` and `track_prompt_evolution()` methods that record prompt version ancestry**
  — source: `prompt-hub/src/lineage.rs`; models already have `VersionRecord` (added PR #32); provenance: TODO.md V section gap + existing VersionRecord struct awaiting usage

---

## P2: Feature flag hygiene (dead flags → dead_code → remove)

Five feature flags in `prompt-hub/Cargo.toml` are **declarations with zero `cfg(feature = "...")` gating any source file** — they gate nothing and should be either wired-in or removed.

- [!] **Audit/resolve dead feature flags: `vibe`, `multimodal`, `chaos`, `chaos-automation`, `tokenizers`**
  — `vibe`: module exists (717 lines) and IS wired via `hub.vibe_code()` but without a feature gate → either add `cfg(feature = "vibe")` or remove from features table
  — `multimodal`: module exists (`multimodal.rs`, 294 lines with real MIME validation logic) but no `#[cfg]` gates it → needs wiring OR removal
  — `chaos` / `chaos-automation`: empty feature arrays `[ ]`, no dependent code at all → candidates for removal
  — `tokenizers`: declared with `dep:tokenizers` dependency but no `cfg(feature = "tokenizers")` anywhere → the dep is consumed by `search.rs` unconditionally (smart-ort path) — either gate via feature or remove from features table
  — source: `prompt-hub/Cargo.toml` feature declarations vs. actual `grep -rl 'cfg(feature ...' src/` results; provenance: self-discovery

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
  — The local CLI was fixed by PR #41 (`cli_identity()` returns `local_operator` with Read+Write+Admin), but the server's `default_agent()` and any direct `PromptHub::new()` without explicit config still produce `anonymous` (Read-only) identity. Any programmatic user who constructs a `PromptHub` directly gets write-denied mutations.
  — source: TODO.md line 20–24, V section; `prompthub-server/src/routes.rs` line ~64 (`default_agent()`); provenance: TODO.md + code inspection

---

## What was built across sessions (merged to main)

### SMART_EMBEDDING EPIC (PRs #44/#45/#46/#47/#48 — complete)
- Extract pluggable Embedder trait + HashEmbedder backend (+7 tests)
- Write prompt embeddings on index via Embedder (storage helpers + integration test)
- Select embedder backend from HubConfig (e2e verified)
- Wire ort-based OrtEmbedder behind smart-ort feature + HubConfig selection
- Real ONNX inference: lazy model download, tokenizers, ort::Session, [CLS] extraction, L2-normalize

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
