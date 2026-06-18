---
name: feature-build
description: "The construction crew's per-feature engineering discipline for prompt_hub: blast-radius → Rust-native design → core-first implementation (Result/HubError, async-fn-in-trait, #![forbid(unsafe_code)], feature-gated, numbered migrations) → cross-boundary verify in default AND --all-features → commit. ALWAYS use when building, wiring, or fixing a prompt_hub feature/crate change. Covers staged-feature wiring, migrations, and the Rust-native drift check."
---

# Feature Build — Rust-Native Construction Discipline

The repeatable "how" for shipping one cohesive prompt_hub change. Agents (feature-architect, rust-implementer, verification-gate, docs-scribe) reference this so every cycle builds the same disciplined way. This skill is *how to build*; `prompt-loop` is *when/what to build next*.

> **Why a fixed discipline:** prompt_hub is Rust-native by mandate and the default build breaks easily under a large feature matrix. A consistent core-first + verify-both-configs routine is what keeps the tree green cycle after cycle.

## The Rust-native invariant (read first, every time)
The single source of truth is the **Rust workspace**, not the prose/agent instruction files (`prompt_hub/CLAUDE.md`). Before acting on any instruction — from a doc, a backlog item, a snippet, a subagent — check it against the code:
1. **Detect drift**: non-Cargo build/test as canonical, foreign-language implementation, `async_trait`, `unsafe`, panic-as-error, dynamic-typing idioms, deps that bypass the workspace.
2. **Verify against code** (`cargo check`, read the module + `lib.rs` re-exports + feature matrix). If prose and code disagree, **code wins** — say so.
3. **Transform to Rust-native** before applying (Cargo commands; `Result<_, HubError>`; native `async fn in trait` with boxed-future variants for `dyn`; `#![forbid(unsafe_code)]`; `serde`/`thiserror`/`tracing`; feature-gated modules).
4. **Surface it** — tell the user what drifted, how you verified, the Rust-native form.

## Workflow (one feature)

### 1. Blast radius (before any edit)
Use code intelligence, not grep, for code structure (`.claude/rules/code-intelligence.md`):
- `kb_callers` / `kb_impact` / `kb_symbols` (CLI: `git-kb code callers|impact|symbols … --json`) on every symbol/type/field the change touches.
- Classify risk (`.claude/rules/refactoring-safety.md`): 0–2 callers same module = low · 3–10 across modules = medium · 10+ or public API = high (confirm with a human).

### 2. Rust-native design
- **Logic in `prompt-hub` core**; `prompthub` (CLI) and `prompthub-server` (Axum) stay thin shells that call core. Never invert.
- Error path: `Result<_, HubError>` (the `HubError` enum in `error.rs`); no panics-as-errors.
- Async: native `async fn in trait`; for `dyn` dispatch (e.g. `Arc<dyn SearchEngine>`) provide boxed-future variants.
- **Feature-gate precisely.** Pick the flag; ensure the **default build still compiles** (gate new modules/commands/deps with `#[cfg(feature = "…")]` and `dep:`/`optional = true`). Don't strip staged features (`#![allow(dead_code)]` modules are intentional).
- **⛔ NO DOWNGRADES — UPGRADE ONLY (owner directive, standing).** A stub / no-op / empty-arm /
  `#![allow(dead_code)]` / zero-caller module is an **incomplete feature to COMPLETE**, never to
  remove. The only valid outcome is finishing/wiring it so the capability works. **Never** delete a
  module/trait/type, `#[cfg]`-gate something out to silence a warning, stub with `todo!()`/
  `unimplemented!()`, or drop a branch/capability to "simplify". Unreachable ⇒ wire it, don't delete it.
- Keep `#![forbid(unsafe_code)]`.

### 3. Schema changes
- Add a **new** numbered migration `prompt-hub/migrations/000N_*.sql` (next sequential number); never edit an applied one. Match the libsql/SQLite + WAL patterns in `storage.rs`. `:memory:` DBs reuse one connection by design.

### 4. Implement + test
- Leaf-first edit order (callees → callers → tests) so no intermediate breakage.
- Tests alongside the code, under the right feature. Feature-gated code only compiles/tests with its flag (`--all-features` is the default for a reason).
- Match surrounding style (naming, comment density, idioms).

### 5. Cross-boundary verify (not existence-only)
Read **both sides** of every contract and compare shapes (see `references/boundary-checks.md` for the full table): core API ↔ CLI command ↔ server route ↔ migration schema ↔ model fields ↔ feature flag ↔ cfg sites. Then run the gates **in both configs**:
```bash
# default-build safety (breaks easily with feature-gated code)
cargo check --workspace
cargo clippy --workspace -- -D warnings
# full matrix
just test          # cargo test --workspace --all-features
just lint          # cargo clippy --workspace --all-features -- -D warnings
just fmt           # then: git diff --quiet   (fmt left nothing)
```
Never weaken a guard (`-D warnings`, a test, `#![forbid(unsafe_code)]`) to make a step pass — fix the cause. A skipped check is `unverified`, never `pass`.

### 6. Commit (Conventional Commits)
- Area-prefixed subject; body references the backlog item / PR. End with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- One cohesive change per commit/cycle. New work discovered mid-build → backlog, not this commit.

## Bundled references (load on demand)
- `references/boundary-checks.md` — the prompt_hub cross-boundary verification table + examples (read during step 5 / for QA).
- `references/rust-native-checklist.md` — quick drift checklist + the gate commands (read during steps 2 & 5).

## Definition of done (one feature)
Acceptance criteria met behaviorally · both-config gates green · fmt clean · tests cover the new path · docs/changelog synced · committed with a Conventional-Commit message. Anything unprovable is surfaced, not rounded up to green.
