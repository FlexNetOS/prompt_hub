# Handoff Packet (latest) — handoff.packet.v2

> Compiled by `hf fleet render prompt_hub` from the FLEET ledger (meta/.handoff) + this repo's git-text capsule/cards. Not rendered from a per-repo ledger (ADR-0004 §3).

## 1. North Star (prompt_hub)
A non-technical user makes any request; prompt_hub transforms, communicates, and delivers it as intended (SwarmBundle -> handoff.task.v1).

## 2. State Precedence
Git > FLEET ledger (meta/.handoff/ledger.db) > tasks/*.task.json > this packet.

## 3. Progress
Done: 27/40.  FLEET tamper-evident events verified: 17.

## 4. Remaining
- [P0] **PHTASK-0028** — Fix default-features build of prompt-hub (argon2 OsRng)
- [P2] **PHTASK-0029** — Decide `defaults.rs` (seed_database no-op)
- [P3] **PHTASK-0030** — Wire or gate `shutdown.rs` (ShutdownCoordinator)
- [P1] **PHTASK-0031** — Complete or remove `multimodal_input.rs`
- [P2] **PHTASK-0032** — Safe plugin discovery for `plugins.rs`
- [P1] **PHTASK-0033** — Wire `templates.rs` TemplateEngine (verify stale claim)
- [P3] **PHTASK-0034** — Make Junie a first-class PromptHub field
- [P1] **PHTASK-0035** — Cover remaining hub methods with server routes
- [P2] **PHTASK-0036** — Move inline CLI commands to dedicated files
- [P2] **PHTASK-0037** — Write real DDL for migration 0008_generation_params
- [P1] **PHTASK-0038** — Add tests to `hooks.rs` (orchestrator path)
- [P1] **PHTASK-0039** — Integration test for hub.get() RBAC+intent flow
- [P3] **PHTASK-0040** — Default identity lacks Write for non-operator callers

