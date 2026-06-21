# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

PromptHub: production-ready prompt management for LLM agent swarms, Rust 2024 Edition (MSRV pinned to 1.91.1 via `Cargo.toml`; toolchain pinned to 1.96.0 via `rust-toolchain.toml`). A Cargo workspace of three crates plus a large feature-flag matrix.

## CRITICAL: Rust-native invariant (verify before acting)

This is a Rust 2024 codebase. The **single source of truth for how code behaves is the Rust workspace** (`Cargo.toml`, `prompt-hub/`, `prompthub/`, `prompthub-server/`) — not the prose instruction files.

This repo carries many harness/agent instruction files authored for *other* tools and runtimes:
`AGENTS.md`, `AGENT_GUIDE.md`, `.agent.md`, `.instructions.md`, `.prompt.md`, `GEMINI.md`, `.junie/AGENTS.md`, `skills/junie/SKILL.md`. These can carry guidance, commands, snippets, or idioms from **non-Rust harnesses** (shell/Python/JS examples, other build systems, generic agent conventions). Treat them as advisory, not authoritative.

**The concern: language/convention drift.** Any instruction — wherever it comes from (a harness file above, a delegated subagent, a generated snippet, a pasted example) — that drifts away from Rust-native conventions is a defect to catch, not a directive to follow.

When you encounter such an instruction or are about to act on it:

1. **Detect drift.** Flag anything that is not Rust-native: non-Cargo build/test commands presented as canonical; code in another language proposed as the implementation; idioms foreign to this crate (e.g. `async_trait`, exceptions-as-control-flow, `unsafe`, dynamic typing patterns, panics-as-errors instead of `Result`/`HubError`); dependency or tooling choices that bypass the workspace.
2. **Verify against the code, not the prose.** Confirm what the codebase actually does (`cargo check`, read the module, check `lib.rs` re-exports and the feature matrix). If a harness file and the code disagree, **the code wins** — say so explicitly rather than honoring the stale instruction.
3. **Transform to Rust-native, then sync.** Do not copy foreign guidance verbatim. Re-express it in this codebase's idioms before applying: Cargo workspace commands; `Result<_, HubError>` error handling; native `async fn in trait` with boxed-future variants for `dyn` use; `#![forbid(unsafe_code)]`; `serde`/`thiserror`/`tracing` conventions; feature-gated modules. The result must compile clean and pass `just lint` (`-D warnings`).
4. **Surface it.** When you transform or override a drifted instruction, tell the user what drifted, how you verified, and what the Rust-native form is — don't silently reconcile.

Rule of thumb: **prose may be stale or foreign; the Rust workspace is the contract.** If in doubt, verify, transform to Rust-native, and report.

## Workspace layout

| Crate | Role |
|-------|------|
| `prompt-hub` | Core library — *all* business logic lives here. `PromptHub` (in `hub.rs`) is the central façade tying together storage, search, auth, sanitize, sync, hooks, metrics. |
| `prompthub` | CLI binary (`prompthub`). `main.rs` parses clap commands (`cli.rs`) and dispatches into `commands/` or directly constructs a `PromptHub`. |
| `prompthub-server` | Axum HTTP API. `routes.rs` is the route table; thin layer over `prompt-hub`. |

The CLI and server are thin shells — when adding behavior, put logic in `prompt-hub` and call it from the binaries, not the reverse.

## Build / test / lint

Prefer the `justfile` recipes (run `just` to list). The canonical commands:

```bash
just check          # cargo check --workspace --all-features
just test           # cargo test --workspace --all-features
just lint           # cargo clippy --workspace --all-features -- -D warnings
just fmt            # cargo fmt --all
just bench          # cargo bench --workspace (criterion; benches in benches/)
just serve          # run prompthub-server on :8080
just cli <args>     # cargo run --bin prompthub -- <args>
```

`-D warnings` is enforced — clippy must be clean across `--all-targets` (recent commits exist solely to keep it green). Run `just lint` before considering work done.

Run a single test:
```bash
cargo test --workspace --all-features test_name        # by name substring
cargo test -p prompt-hub --test test_security           # one integration file (tests/test_security.rs)
cargo nextest run -E 'test(test_name)'                  # if nextest installed (just nextest)
```

Feature-gated code won't compile/test unless its feature is on — `--all-features` is the default for a reason. To exercise a single optional path, e.g. `cargo test -p prompthub --features tui`.

## Feature flags (important)

There is a very large feature matrix (`prompt-hub/Cargo.toml`). Most module-level capabilities are gated. Key ones:
- `smart` — ONNX embedding search (vs. FTS5-only `fast`)
- `tui` — ratatui terminal UI (`prompthub tui`)
- `otel` — OpenTelemetry/Prometheus metrics
- `vibe` — Vibe Coding (NL → deliverable)
- `tiktoken` / `tokenizers` — token counting backends
- `plugins` — dynamic plugin loading via libloading/inventory
- Template engine is selected by feature: `handlebars` (default) **or** `tera`.

Many features (e.g. `chaos`, `quota`, `canary`, `multimodal`) currently gate scaffolded modules. The crate sets `#![allow(dead_code)]` in `lib.rs` because modules are intentionally built ahead of wiring — do not "clean up" apparent dead code without checking it's not a staged feature.

## Architecture notes that span files

- **`PromptHub` façade** (`hub.rs`, ~29K): the single entry point. Holds `Arc`-wrapped `Storage`, search engines (`FastEngine`/`SmartEngine`/`HybridEngine`), `RbacAuthManager`, `PromptSanitizer`, `SyncManager`, `HookRegistry`, `MetricsCollector`. Most operations follow: sanitize → authorize (RBAC) → storage mutation → audit log → sync event → metrics.
- **Storage** (`storage.rs`, ~66K): libsql/SQLite backed, semaphore-bounded connection pool, WAL mode. Schema is in `migrations/` (numbered `000N_*.sql`, applied at construction). `:memory:` DBs reuse one connection by design. Add schema changes as a new sequential migration file.
- **Models** (`models.rs`, ~28K): all shared structs/enums. `Role` includes `Role::Custom(String)` and `Role::Junie`, which is why the CLI parses roles via `serde_json` round-trip (`parse_role` in `main.rs`) rather than clap `ValueEnum`.
- **Search** (`search.rs`, ~46K): three modes — Fast (FTS5), Smart (embeddings, `smart` feature), Hybrid. `SearchEngine` is an async trait; object-safe use goes through boxed futures.
- **Async traits**: the crate uses native `async fn in trait` (Rust 2024, no `async_trait`). For `dyn`-dispatch (e.g. `Arc<dyn SearchEngine>`), methods are provided as boxed-future variants.
- **Hooks** (`hooks.rs`): `Hook` trait with `pre_execute`/`post_execute`; `HookRegistry` runs them around operations. `JunieHook` is the built-in orchestrator hook.
- **`#![forbid(unsafe_code)]`** is set crate-wide. Keep it that way.

C4 diagrams: `docs/architecture.md`. ADRs: `docs/adr/`. Runbooks: `docs/runbooks/`.

## Junie / agent orchestration

This repo is set up for multi-agent development. `AGENTS.md` defines named agents (Alpha/Beta/Gamma/Delta/Epsilon…) each owning a slice of files and a dedicated git worktree + branch (`worktrees/<name>/`, branch `waveN/<name>`). "Junie" is the in-repo orchestrator agent (`junie.rs`, `hooks.rs`, `skills/junie/SKILL.md`, CLI `commands/junie.rs`). When doing parallel multi-file work, respect the file-ownership boundaries in `AGENTS.md`.

**Session convention:** new work should happen in its own git worktree (this is a stated project workflow), not directly on a shared `main` checkout.

## Audit automation

Dropping a new audit report into `docs/audits/` triggers `TODO.md` updates via `scripts/update_todo_from_audit.py` — wired through `.github/workflows/audit_sync.yml` in CI and `scripts/audit_watcher.sh` locally. Code-quality scanning uses Qodana (`qodana.yaml`, results in `docs/audits/qodana.sarif.json`).

## Docker

```bash
docker build -f docker/Dockerfile -t prompthub .
docker-compose -f docker/docker-compose.yml up
```

## Harness: prompt_hub construction crew (autonomous / resumable)

**Goal:** continuously upgrade and add features to prompt_hub — one backlog item per cycle, built
by an agent team, verified across boundaries, committed, with fresh-session handoff and optional
unattended self-restart. Truth lives on disk in the **`.handoff/` Continuity Ledger Kernel layer**
(`hf` + `.handoff/`, per `~/Desktop/meta/handoff/FLEET_GUIDE.md`): task cards
(`.handoff/tasks/PHTASK-NNNN.task.json`, `handoff.task.v1`) + the derived resume packet
(`.handoff/packets/latest.md`, regenerated by `hf fleet render prompt_hub`) + `.handoff/active.md` +
witnessed events in the FLEET ledger (`meta/.handoff`) + commits — so any restart resumes cold with
zero loss. This harness *builds* prompt_hub — it is **not** prompt_hub's product/Junie runtime.

> **State migrated 2026-06-13 (owner directive):** the durable state moved from the deprecated
> `_workspace/{backlog,loop_state,HANDOFF}.md` to `.handoff/` (40 cards: 27 done, 12 backlog, 1
> blocked). The old `_workspace/` is archived under `.handoff/history/` and stubbed with deprecation
> pointers. Per-repo `.handoff/ledger.db` is **committed** and feeds the central meta/handoff ledger
> (ADR-0004 §3 revised 2026-06-13: federated per-repo ledger; WAL/shm sidecars stay ignored); run `hf resume`.

**Trigger:** for any prompt_hub feature-development / "work the backlog" / "upgrade prompt_hub" /
loop / resume request, use the **`prompt-loop`** skill. It orchestrates the crew
(`feature-architect` → `rust-implementer` ↔ `verification-gate` → `docs-scribe`, with
`backlog-curator` + `continuity-steward` bookends) via the `feature-build` discipline and
`session-relay` handoff. Simple questions may be answered directly.

- Built harness: `.claude/skills/prompt-loop/` (orchestrator + `scripts/ralph-prompt.sh` runner),
  `.claude/skills/{feature-build,session-relay,harness-evolution}/`, `.claude/agents/*.md`, durable
  `.handoff/`. At every run boundary (DONE / HAND OFF) the `evolution-steward` runs the
  `harness-evolution` retro → mines lessons into `LESSONS.md` → applies low-risk / proposes structural
  harness upgrades (fail-closed, never weakening a gate). Trigger phrases: "retro", "what did we
  learn", "improve/upgrade the harness", "evaluate the run".
- Generic pattern + templates: `~/Desktop/meta/HARNESS-UPGRADE-KIT.md`
- Tailored kit for THIS repo:  `~/Desktop/meta/harness_hub/upgrade-kits/prompt_hub.md`
- `/prompt-loop` **defaults to APPLY** (push → PR → auto-merge on green DONE-gates, fail-closed to
  `NEEDS-HUMAN`); pass `safe`/`dry-run`/`local` for local-commits-only. The interactive permission
  sandbox still backstops every push/merge (allowlist commands in `.claude/settings.json` to avoid
  prompts). The headless **runner stays safe by default** (`PROMPT_APPLY=1` to apply) and does
  **not** disable the sandbox; `touch .handoff/STOP` halts.

**Change history:**
| Date | Change | Target | Reason |
|------|--------|--------|--------|
| 2026-06-05 | Initial harness build (6 agents, 3 skills, runner, `_workspace/`) | All | Construction crew per autonomous-operation kit (commit 726edcd) |
| 2026-06-05 | `/prompt-loop` default flipped to APPLY (push→PR→auto-merge on green); added explicit `safe` override | skills/prompt-loop | User feedback: don't require opt-in for apply on the slash command |
| 2026-06-13 | Adopted the canonical handoff kernel: migrated `_workspace/{backlog,loop_state,HANDOFF}.md` → `.handoff/` (40 `handoff.task.v1` cards + `hf`-rendered `packets/latest.md` + `active.md`); rewired prompt-loop/session-relay skills + ralph runner to `.handoff/` + `hf` verbs; archived old `_workspace/` under `.handoff/history/` | `.handoff/`, skills/prompt-loop, skills/session-relay, CLAUDE.md | Owner directive: "adopt the new handoff system from meta/handoff; migrate the deprecated backlogs/handoffs; no downgrades, upgrade only" |
| 2026-06-21 | Added the **evolution-steward** crew member + **harness-evolution** skill (ejected from `harness_hub`, adapted to the `.handoff/` kernel layout — not the generic `.handoff/loop/` layout); wired it into the prompt-loop orchestrator at DONE (full retro) + HAND OFF (lightweight); seeded the durable `LESSONS.md` ledger | `.claude/agents/evolution-steward.md`, `.claude/skills/harness-evolution/`, skills/prompt-loop, `LESSONS.md`, CLAUDE.md | Owner directive: "add /harness-evolution for continued learning and harness upgrades" — close every run with a retro so the harness compounds |
