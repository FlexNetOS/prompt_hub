# TODO — prompt_hub v4.6

> Prioritized, actionable items. Each has a file path and a specific task. Checked items = done.

## P0 — Compilation Blockers (do first)

- [ ] **Verify `cargo check` passes** — Run in environment with rustc. Fix first N errors.
  - `cd /mnt/agents/output/project && cargo check 2>&1 | head -50`
  - File: all crates

- [ ] **Fix `AgentIdentity::default()` usage** — `hub.rs` calls `AgentIdentity::default()` but struct may not derive Default.
  - File: `prompt-hub/src/models.rs` line ~141
  - Fix: Add `#[derive(Default)]` to `AgentIdentity` or write manual impl

- [ ] **Make `storage.row_to_prompt()` accessible to search engines** — It's private, FastEngine/SmartEngine need it.
  - File: `prompt-hub/src/storage.rs` line ~180
  - Fix: Change `fn row_to_prompt` to `pub(crate) fn row_to_prompt`

- [ ] **Add missing `use` imports in `routes.rs`** — `Server`, `State`, `Arc` types may need explicit imports.
  - File: `prompthub-server/src/routes.rs`
  - Fix: Add `use axum::extract::State; use std::sync::Arc;`

- [ ] **Add `#[cfg(feature = "handlebars")]` guards in `templates.rs`** — HandlebarsEngine uses `handlebars` crate behind feature flag.
  - File: `prompt-hub/src/templates.rs`
  - Fix: Wrap `#[cfg(feature = "handlebars")]` around `handlebars_engine` module

## P1 — Feature Completeness

- [ ] **Add `hub.list()` method** — Used by routes.rs `list_prompts` handler but may not exist.
  - File: `prompt-hub/src/hub.rs`
  - Fix: Add `pub async fn list(&self, pagination: Pagination) -> Result<Paginated<Prompt>>`

- [ ] **Add `storage.list_prompts()` method** — Needed by `hub.list()`.
  - File: `prompt-hub/src/storage.rs`
  - Fix: Add `pub async fn list_prompts(&self, page: usize, per_page: usize) -> Result<Paginated<Prompt>>`

- [ ] **Verify `storage.log_audit()` is called by all mutating hub methods** — audit_trail needs data.
  - File: `prompt-hub/src/hub.rs`
  - Check: `register()`, `update()`, `rollback()`, `lock()`, `unlock()`, `transfer_ownership()`, `evolve_prompt()`

- [ ] **Fix `canary.rs` sha2 import** — `sha2::Sha256::digest` needs `use sha2::{Sha256, Digest};`
  - File: `prompt-hub/src/canary.rs`

- [ ] **Add `pub mod` declarations for 12 new wave-6 modules** — circuit_breaker, budget, quota, moderation, retention, garbage_collector, load_balancer, provider_health, satisfaction, analytics, diff, lineage.
  - File: `prompt-hub/src/lib.rs`
  - Check: Are all 12 declared? Currently canary.rs is declared. Add the rest.

## P2 — Quality

- [ ] **Run `cargo clippy --workspace --all-features -- -D warnings`**
  - Fix all warnings (unused imports, redundant clones, etc.)

- [ ] **Run `cargo fmt --all -- --check`**
  - Format all files

- [ ] **Run `cargo doc --workspace --all-features --no-deps`**
  - Fix documentation warnings

- [ ] **Verify `#![forbid(unsafe_code)]` on all 49 library modules**
  - `grep -r "forbid(unsafe_code)" prompt-hub/src/ | wc -l` should be 49

## P3 — Testing

- [ ] **Run `cargo test -p prompt-hub --lib`**
  - Fix failing tests

- [ ] **Run `cargo test --workspace`**
  - Fix integration test failures

- [ ] **Add edge case tests for sanitization**
  - Zero-width characters, RTL override, homoglyphs
  - File: `prompt-hub/src/sanitize.rs` test module

- [ ] **Add concurrency tests for LockManager**
  - Multiple agents racing for same prompt
  - File: `prompt-hub/src/lock.rs` test module

## P4 — Documentation

- [ ] **Complete API documentation for all 20 Hub methods**
  - Add doc comments with examples
  - File: `prompt-hub/src/hub.rs`

- [ ] **Document feature flags table in README.md**
  - Map each flag to module and use case

- [ ] **Add crate-level docs in lib.rs**
  - `//!` doc comment with quickstart example

## P5 — Polish (last)

- [ ] **Verify Docker build works**
  - `docker build -f docker/Dockerfile -t prompthub:test .`

- [ ] **Verify CI workflow passes**
  - Check `.github/workflows/ci.yml` syntax

- [ ] **Add git-cliff configuration** (`.cliff.toml`)
  - For automated changelog generation

## Done

- [x] 49 library modules with real logic
- [x] 50 types in models.rs
- [x] 20 Hub API methods
- [x] 13 HTTP routes with real PromptHub state
- [x] 36 CLI commands calling real methods
- [x] 600+ test functions
- [x] 9 SQL migrations
- [x] Remove `async_trait` (Rust 2024 native)
- [x] Remove `optional = true` from workspace deps
- [x] Write SESSION.md
- [x] Write TODO.md
- [x] Write AGENT_GUIDE.md
