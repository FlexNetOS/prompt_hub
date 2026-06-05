# TODO — prompt_hub v4.6

> Prioritized, actionable items. Each has a file path and a specific task. Checked items = done.

## P0 — Compilation Blockers (do first)

- [x] **Verify `cargo check` passes** — ✅ GREEN as of 2026-06-05 (PR #30).
  - A regression appeared post-merge: dependabot's `sha2 0.11` bump broke `audit.rs:75`
    (`finalize()` no longer impls `LowerHex`, E0277). Fixed by hand hex-encoding the digest
    (byte-identical). `cargo check --workspace --all-features` → 0; 671 tests pass; clippy/fmt clean.

## V — Verification findings (2026-06-05, prompt-loop `/verify`)

- [ ] **Route CLI tracing logs to stderr (`prompthub metrics` emits logs on stdout).**
  - `/verify` found `prompthub metrics` prints the Prometheus exposition AND ~14 ANSI INFO log
    lines on the **same stdout stream**, so `prompthub metrics > out.prom` is not parser-clean
    (stderr was empty; `RUST_LOG=error` works around it).
  - File: `prompthub/src/main.rs:35` — add `.with_writer(std::io::stderr)` to `tracing_subscriber::fmt()`.

- [ ] **CLI mutations unusable out-of-the-box — default identity lacks `Write`.**
  - `prompthub add` (and all writes) fail: `Unauthorized: agent 'anonymous' lacks capability Write`.
    Provide a configured local identity / `login` flow / dev-capability default for the CLI.
  - File: `prompthub/src/commands/add.rs:28` (`AgentIdentity::default()`); RBAC in `prompt-hub/src/auth.rs`.
  - Pre-existing; blocks the entire write surface (incl. observing the audit `diff_hash` via CLI).

- [ ] **Regenerate `docs/audits/qodana.sarif.json`** — committed SARIF (2026-06-04) is stale.
  - Re-run the CI Qodana job and commit fresh output; 60 of 87 findings are obsolete and line numbers drifted.

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

- [x] **Run `cargo clippy --workspace --all-features -- -D warnings`** — ✅ clean (2026-06-05).
  - Also fixed 18 `unused_qualifications` via `cargo fix` (PR #32, qodana triage).

- [x] **Run `cargo fmt --all -- --check`** — ✅ clean (2026-06-05).

- [ ] **Run `cargo doc --workspace --all-features --no-deps`**
  - Fix documentation warnings (now unblocked — was blocked by the P0 build error). Next-to-build.

## Audits

- [x] **Review audit findings from `qodana.sarif.json`** — ✅ triaged 2026-06-05 (PR #32).
  - 18 live `RsUnnecessaryQualifications` fixed; rest stale/already-fixed or subjective won't-fix.
  - ⚠️ The SARIF is now stale — see **V** section: regenerate it before the next triage.
  - File: `docs/audits/qodana.sarif.json`
## P3 — Testing

- [x] **Run `cargo test -p prompt-hub --lib`** — ✅ green (part of 671-test workspace pass, 2026-06-05).

- [x] **Run `cargo test --workspace`** — ✅ 671 passed / 0 failed (`--all-features`, 2026-06-05).

- [x] **Add edge case tests for sanitization** — ✅ landed (PR #27, commit `72de246`).
  - Zero-width (ZWSP/ZWNJ/ZWJ/BOM), RTL/LTR overrides, homoglyphs, negative cases.
  - File: `prompt-hub/src/sanitize.rs` test module

- [x] **Add concurrency tests for LockManager** — ✅ landed (PR #27, commit `72de246`).
  - 32 racing agents → unique tokens; verify-only-holder; heartbeat clamp.
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
