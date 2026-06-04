# TODO — prompt_hub v4.6

> Prioritized, actionable items. Each has a file path and a specific task. Checked items = done.

## P0 — Compilation Blockers (do first)

- [ ] **Verify `cargo check` passes** — Requires rustc in environment (not available in sandbox).
  - All *known* blockers below are fixed. Next agent should run `cargo check`.

- [x] **Fix `AgentIdentity::default()` usage** — Already had manual Default impl.
  - File: `prompt-hub/src/models.rs` lines 146-156
  - Status: Was already correct (manual impl, not derive)

- [x] **Fix routes.rs `default_agent()` capabilities type** — `Vec<String>` → `Vec<Capability>`
  - File: `prompthub-server/src/routes.rs` line 64
  - Changed: `vec!["read".to_string(), "write".to_string()]` → `vec![Capability::Read, Capability::Write]`

- [x] **Fix canary.rs sha2 import** — Added `use sha2::{Sha256, Digest};`
  - File: `prompt-hub/src/canary.rs` lines 5, 27
  - Changed: `sha2::Sha256::digest(...)` → `Sha256::digest(...)`

- [x] **Fix hub.rs tests** — `test_agent()` capabilities + `test_prompt()` fields + `Role::User`
  - File: `prompt-hub/src/hub.rs` lines 583-634
  - Changed: capabilities type, all Prompt fields corrected, `Role::User` → `Role::Developer`

- [x] **Add missing `Prompt::new()` constructor** — storage.rs test calls it
  - File: `prompt-hub/src/models.rs` lines 400-426
  - Added: `pub fn new(name: &str, system_prompt: &str) -> Self`

- [x] **Fix storage.rs test `.is_active()`** — method doesn't exist on Prompt
  - File: `prompt-hub/src/storage.rs` line 1434
  - Changed: `assert!(fetched.is_active())` → `assert_eq!(fetched.status, Status::Active)`

- [x] **Add 10 missing HubError variants** — used across codebase but never defined
  - File: `prompt-hub/src/error.rs` lines 32-60
  - Added: StorageError, AuthError, LockError, SearchError, BadRequest, AuditError, ValidationError, SerdeError, SyncError, SanitizationError

- [x] **Fix hub.rs LockError struct literal** — LockError is String, not struct
  - File: `prompt-hub/src/hub.rs` lines 265-268
  - Changed: struct literal `{ prompt_id, held_by }` → `LockError(format!("..."))`

- [x] **Fix auth.rs error construction + test** — Unauthorized is tuple, not struct
  - File: `prompt-hub/src/auth.rs` lines 109-111, 371, 162
  - Changed: struct literal → tuple, pattern match, added missing RateLimited arg

- [x] **Add missing `VersionRecord` struct** — used in storage.rs, never defined
  - File: `prompt-hub/src/models.rs` lines 428-438
  - Added: `pub struct VersionRecord { id, prompt_id, parent_id, version, changelog, diff, created_at }`

- [x] **Fix auth.rs `crate::error::AgentIdentity`** — AgentIdentity is in models, not error
  - File: `prompt-hub/src/auth.rs` line 111
  - Changed: `crate::error::AgentIdentity { id, name }` → format string for `Unauthorized(String)`

## P1 — Feature Completeness

- [x] **Add `hub.list()` method** — Already exists at lines 212-226.
  - File: `prompt-hub/src/hub.rs`

- [x] **Add `storage.list_prompts()` method** — Already exists at lines 584-636.
  - File: `prompt-hub/src/storage.rs`

- [x] **Verify `storage.log_audit()` called by mutating hub methods** — All 7 methods call it.
  - Methods: register, update, rollback, lock, unlock, transfer_ownership, evolve_prompt

- [x] **Add `pub mod` declarations for 12 wave-6 modules** — All 12 declared in lib.rs.
  - File: `prompt-hub/src/lib.rs`

- [x] **Make `storage.row_to_prompt()` accessible** — Already `pub(crate)` at line 1219.
  - File: `prompt-hub/src/storage.rs`

- [x] **Add `#[cfg(feature = "handlebars")]` guards in templates.rs** — Already present at lines 51, 95.
  - File: `prompt-hub/src/templates.rs`

## P2 — Quality

- [x] **Verify `#![forbid(unsafe_code)]` on all 49 library modules** — 49/49 confirmed.

- [ ] **Run `cargo clippy --workspace --all-features -- -D warnings`**
  - Fix all warnings (unused imports, redundant clones, etc.)

- [ ] **Run `cargo fmt --all -- --check`**
  - Format all files

- [ ] **Run `cargo doc --workspace --all-features --no-deps`**
  - Fix documentation warnings

## Audits

- [ ] **Review audit findings from `qodana.sarif.json`** — Audit dropped at 2026-06-03 20:00. Found 87 issues (40
  warning, 47 note).
  - File: `docs/audits/qodana.sarif.json`
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
- [x] 50 types in models.rs (+ VersionRecord = 51)
- [x] 20 Hub API methods
- [x] 13 HTTP routes with real PromptHub state
- [x] 36 CLI commands calling real methods
- [x] 600+ test functions
- [x] 9 SQL migrations
- [x] Remove `async_trait` (Rust 2024 native)
- [x] Remove `optional = true` from workspace deps
- [x] 27 HubError variants (17 original + 10 added)
- [x] Write SESSION.md
- [x] Write TODO.md
- [x] Write AGENT_GUIDE.md
- [x] Wave 9: Fix all known compilation blockers (13 fixes)
