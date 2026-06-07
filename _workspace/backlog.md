# prompt-loop backlog — prompt_hub construction crew

The **single source of truth** for what the crew builds next. Legend:
`- [ ]` todo · `- [x]` done+verified · `- [!] blocked: <reason>`.
Each item = one cohesive, shippable unit sized to one cycle. Every item cites its source.

> Curated by `backlog-curator` at DISCOVER on 2026-06-05 from **real repository state**
> (verified via `cargo check/doc/tree`, `git log origin/main`, `gh pr list`, qodana SARIF,
> `prompt-hub/Cargo.toml` feature matrix). Seed placeholders replaced; `[x]`/`[!]` history preserved.
> `gh` was authenticated; no offline gaps. Rust-native invariant (`prompt_hub/CLAUDE.md`) applies to
> every item: wire features behind their flag with tests; foreign/non-Cargo guidance is drift to fix.

---

## ✅ P0 — Build RED → GREEN (fixed in cycle 1)

- [x] **Fix `audit.rs:75` sha2 0.11 `LowerHex` breakage — restore a green `--all-features` build.**
      _Cycle 1 (2026-06-05). Fixed Rust-native: hand hex-encode the `hybrid_array::Array` digest via
      `write!(.., "{byte:02x}")` (no new dep), keeping output byte-identical so the hash chain still
      verifies. Gates re-run green across the boundary: `cargo check --workspace --all-features` (0),
      `cargo clippy --workspace --all-features -- -D warnings` (0), `cargo fmt --all -- --check` (clean),
      `cargo test --workspace --all-features` (577+ pass / 0 fail). Landed via the cycle-1 PR._
      The workspace dep was bumped to `sha2 = "0.11.0"` (dependabot PR #11, merged 2026-06-05
      16:38, *after* the last green 652-test build). In sha2 0.11 `Sha256::finalize()` returns
      `hybrid_array::Array<u8, …>`, which does **not** implement `LowerHex`, so
      `format!("{:x}", hasher.finalize())` at `prompt-hub/src/audit.rs:75` fails to compile.
      `cargo check -p prompt-hub` (even default features) and `cargo check --workspace --all-features`
      are both RED on this one site. Fix Rust-native: hex-encode the finalized bytes explicitly
      (e.g. `use base16ct`/manual `write!` over the byte slice, or `hex::encode`) — `canary.rs`
      indexes `digest()[0]` and is unaffected, so this is the *sole* breakage.
      _Source: `cargo check --workspace --all-features` (error E0277 at audit.rs:75); `cargo tree -i sha2`
      shows direct `sha2@0.11.0`; `Cargo.toml:80 sha2 = "0.11.0"`. Smallest possible cycle; unblocks all gates._

## Core library (prompt-hub)

- [x] **Triage the live qodana code-quality findings (24 items) — clippy-clean cleanup.**
      _Cycle 3 (2026-06-05). Triaged all 27 code-smell findings against the CURRENT tree using the
      compiler as ground truth (`-W unused_qualifications`) rather than the stale SARIF line numbers.
      FIXED: the 18 still-live `RsUnnecessaryQualifications` (4 of 22 were already fixed) via
      `cargo fix` — `hub.rs` ×2, `search.rs` ×15, `budget.rs` ×1 (then `cargo fmt`). STALE/already
      fixed (verified, no action): `RsUnwrap` (server `main.rs` no longer `.unwrap()`s — uses `?`),
      `RsAssertEqual` (no `assert!(a==b)` candidate left in `load_balancer.rs`), `RsUnreachablePatterns`
      (rustc's deny-by-default `unreachable_patterns` is silent — build green). WON'T-FIX (subjective
      RustRover style, not a clippy violation, behavior-risk): 2× `RsLift` (`moderation.rs`,
      `sanitize.rs`). Gates green: 0 residual unused-qualifications, clippy -D warnings clean, fmt clean,
      671 tests / 0 fail. Note: the 39 `CargoUnusedDependency` + 21 `NewCrateVersionAvailable` SARIF
      findings remain stale (PR #27 dep removal + dependabot); recommend regenerating the SARIF. Landed via the cycle-3 PR._
      The SARIF (generated 2026-06-04 00:11) reports 87 results, but the 39 `CargoUnusedDependency`
      and 21 `NewCrateVersionAvailable` findings are **stale** — PR #27 (merged 2026-06-04 21:16)
      already removed 32 unused deps, and dependabot owns version bumps. The genuinely actionable,
      shippable-in-one-cycle subset is the Rust code smells: **22 `RsUnnecessaryQualifications`,
      2 `RsLift`, 1 `RsUnwrap`, 1 `RsAssertEqual`, 1 `RsUnreachablePattern`**. Apply the fixes
      and confirm `just lint` stays green. (Do this AFTER the P0 build fix so clippy can actually run.)
      _Source: `docs/audits/qodana.sarif.json` (87 results: 40 warning / 47 note; rule histogram
      via the SARIF). TODO.md "Audits" line. Verify each against current tree — many may already be fixed._

### Epic: wire `smart` embedding search end-to-end — SCOPED into slices (session-3 cycle-3)

> Decomposed by `feature-architect` 2026-06-06 → full plan in `_workspace/s3c3_architect_plan.md`.
> Key findings: the SMART path is **not** actually behind the `smart` feature (it runs under
> `default`; `ndarray` is unused), and there is **no embedding-write path** (the `embeddings` table
> is empty in production — only tests populate it). Build slices 1→3 in order; 4→5 are blocked on an
> inference-runtime decision. Each slice must be an independently-green, mergeable PR.

- [x] **Slice 1 — `refactor(search): extract pluggable Embedder trait + HashEmbedder backend`.**
      _Cycle 10 (2026-06-07). Extracted object-safe `Embedder` trait (boxed-future, `Result<_, HubError>`)
      + `HashEmbedder` backend. `SmartEngine` holds `embedder: Arc<dyn Embedder>`; keeps
      `SmartEngine::new(model_name, storage, dim)` for back-compat. 7 new unit tests (determinism,
      dimension, range, cosine-self≈1.0, object-safety, embedder accessor, mock_embed compat, default_model).
      Gates green: check/clippy(-D warnings)/fmt/682 tests. Landed via PR #44._

- [x] **Slice 2 — `feat(search): write prompt embeddings on index via Embedder`** (deps: Slice 1).
      _Cycle 10 (2026-06-07). Added `Storage::upsert_embedding(prompt_id, &bytes)` + `delete_embedding()`.
      Replaced SmartEngine `index` stub: extracts name+system_prompt+user_template → embeds via
      `Embedder` → persists as LE f32 blob with ON CONFLICT upsert. `remove` deletes embedding row.
      Fixed FK failures in existing tests (prompt must exist before embedding). Added e2e integration
      test (embed→search finds it→remove clears it). Gates green: check/clippy(-D warnings)/fmt/683 tests.
      Landed via PR #45._

- [ ] **Slice 3 — `feat(config,hub): select embedder backend from HubConfig`** (deps: Slices 1-2).
      Build `Arc<dyn Embedder>` from config in `hub.rs:108-119` (default `HashEmbedder` with
      `config.embedding_dimension`); optional `lib.rs` re-export of `Embedder`/`HashEmbedder`.
      Acceptance: `PromptHub::new` default config → register→search returns the prompt end-to-end.
      Same 4 gates. Risk: Low-Medium (touches `lib.rs` re-exports + `HubConfig`; keep default
      deterministic so hub tests stay green).

- [!] **blocked: Slice 4 — `feat(search): gate real-model embedder backend behind `smart` (scaffold)`.**
      Needs the inference-runtime decision (see plan "Open decisions"). Make `smart` meaningful: a
      `#[cfg(feature="smart")]` trait-conformant scaffold returning `HubError` "not configured" (no
      model load yet), so `--features smart` compiles + the contract is tested. Do NOT add a heavy/
      native/download dep in the loop without sign-off. Deps: Slices 1-3.

- [!] **blocked: Slice 5 — `feat(smart): real ONNX/model loading + download + checksum`.**
      Implement `load_model`/`download_model`/`verify_checksum` (`search.rs:271-309`) against the
      chosen runtime; real `Embedder::embed`. Cannot be a pure-CI-green slice (network/model) — ships
      with a network-skipping `#[ignore]` test. Blocked on Open decisions; needs human approval.

> **Open decisions blocking slices 4-5** (from the plan): inference runtime (ort/candle/fastembed/
> remote API — dep weight + `unsafe` FFI vs `#![forbid(unsafe_code)]`); tokenizer source; model
> acquisition + CI network policy; dimension authority (384 fixed vs configurable → migration);
> future `smart`-feature semantics (+ whether `ndarray` stays). Surface to the user, don't guess.

## CLI / server (prompthub, prompthub-server)

- [x] **Add a `prompthub metrics` CLI subcommand that prints the Prometheus exposition.**
      _Cycle 2 (2026-06-05). Added `Commands::Metrics` (cfg `otel`) → `commands::metrics::run()`
      which calls `hub.metrics().prometheus_text()` and prints the v0.0.4 exposition to stdout.
      ⚠️ CORRECTION (post-`/verify` 2026-06-05): the exposition is valid/complete, but the default
      invocation also writes tracing INFO logs to **stdout** (not stderr) — see the follow-up item
      below. Reuses the landed otel path; CLI `otel` feature already forwarded to
      `prompt-hub/otel`, so no Cargo changes. Tests: `test_cli_parse_metrics`,
      `metrics_renders_valid_exposition` (asserts HELP/TYPE preamble). Gates green
      (check default+otel+all-features, clippy -D warnings, fmt, 577+ workspace tests / 0 fail);
      functional run emitted 38 HELP/TYPE lines. Landed via the cycle-2 PR._
      The server already exposes `/metrics` (`prompthub-server/src/routes.rs:559 prometheus_metrics`
      → `render_metrics` → `metrics.prometheus_text()`), but the CLI has **no** way to read metrics
      (`grep metric prompthub/src` finds only `PromptMetrics::default()`). Add a thin `Metrics`
      subcommand in `cli.rs` that calls `hub.metrics().prometheus_text()` (gate behind `otel`,
      matching the server). Small, user-facing, Rust-native, exercises the just-landed otel path.
      _Source: `routes.rs:554-588`; `cli.rs` enum `Commands` has no metrics variant; otel landed via PR #28._

- [x] **Route CLI tracing logs to stderr so stdout stays machine-readable (`prompthub metrics` fix).**
      _Cycle 4 / session-2 cycle-1 (2026-06-05). Fixed Rust-native at `prompthub/src/main.rs`: the
      `tracing_subscriber::fmt()` builder now sets `.with_writer(std::io::stderr)` (logs → stderr,
      stdout reserved for data) and `.with_ansi(std::io::stderr().is_terminal())` (no ANSI escapes
      when stderr is redirected; `std::io::IsTerminal`, stable). Added a subprocess regression test
      `prompthub/tests/cli_log_routing.rs` (gated `otel`, runs in a tempdir): asserts `prompthub
      metrics` stdout starts with the `# HELP`/`# TYPE` Prometheus preamble, contains 0 `INFO`
      lines / 0 ANSI escapes, and that the `info!` lands on stderr. Added `[dev-dependencies]`
      (assert_cmd/predicates/tempfile, workspace-pinned) — prompthub had none. Functional proof:
      `prompthub metrics 2>/dev/null | head -1` → `# HELP prompt_hub_active_locks …`; stdout INFO
      count 0 (was ~14). Gates green across the boundary: check/clippy(-D warnings)/fmt clean,
      672 tests / 0 fail (+1 new test). Landed via the cycle PR._
      `/verify` found that `prompthub metrics` writes its Prometheus exposition AND ~14 ANSI-colored
      tracing INFO lines to the **same stream (stdout)** — `prompthub metrics > out.prom` produces a
      file a Prometheus parser chokes on. STDERR was empty; clean output only via `RUST_LOG=error`
      / `--log-level error`. Root cause: `prompthub/src/main.rs:35` `tracing_subscriber::fmt().init()`
      defaults to stdout, and `--log-level` defaults to `info`. Fix Rust-native: add
      `.with_writer(std::io::stderr)` to the fmt subscriber (and consider disabling ANSI when stdout
      isn't a TTY). This is correct CLI hygiene for any data-on-stdout command, not just `metrics`.
      _Source: `/verify` 2026-06-05 (split-stream capture: 14 INFO lines on stdout, 0 on stderr);
      `main.rs:32-38`. Verify: `prompthub metrics 2>/dev/null | head -1` → first line is `# HELP …`._

- [x] **Make the CLI usable out-of-the-box for mutations (default identity lacks `Write`).**
      _Cycle 7 / session-3 cycle-1 (2026-06-06). Fixed Rust-native without weakening RBAC. Design
      decision: the local CLI operates on its own on-disk store as the trusted owner, so it now acts
      as a **local operator** instead of the capability-less `anonymous` default. Added
      `AgentIdentity::local_operator(name)` (prompt-hub/src/models.rs) granting `[Read, Write, Admin]`
      — together these cover every `Action` (read; write/lock/evolve; delete/admin/transfer). RBAC
      (`RbacAuthManager::authorize_action`) is UNCHANGED and remains the enforcement point; swarm/
      remote/automation identities still need their own explicit grants. Added a single CLI chokepoint
      `prompthub/src/identity.rs::cli_identity()` (display name overridable via `PROMPTHUB_AGENT` env
      for audit attribution) and replaced all 9 `AgentIdentity::default()` call sites in the CLI
      (main.rs ×5 acting identities, add.rs ×2 incl. author, import.rs, fuzzy.rs author). Tests:
      unit `test_agent_identity_local_operator_has_owner_capabilities` (prompt-hub); integration
      `cli_add_identity.rs` (×2: `add` succeeds out-of-the-box + `PROMPTHUB_AGENT` override). Functional
      proof: `prompthub add` in a clean dir → exit 0, "Registered prompt <uuid>" (was exit 1
      Unauthorized). Gates green across the boundary: check/clippy(-D warnings)/fmt clean, 675 tests /
      0 fail (+3). Landed via the cycle PR._
      `/verify` found `prompthub add` (and any mutating command) fails with
      `Error: Unauthorized: agent 'anonymous' lacks capability Write` because the CLI constructs an
      `AgentIdentity::default()` (no capabilities). A first-time user cannot create/update a prompt
      from the CLI at all. Decide + implement the intended path: a configured local identity
      (token/capabilities via `HubConfig`/env), a `prompthub login`/identity flag, or a
      developer-capability default for the local CLI. Pre-existing (not introduced this session),
      but it blocks the whole write surface — including observing the audit `diff_hash` chain via CLI.
      _Source: `/verify` 2026-06-05 (`prompthub add` exit 1, RBAC deny in `auth.rs::authorize_action`);
      `prompthub/src/commands/add.rs:28 AgentIdentity::default()`. Verify: `prompthub add <file>` registers a prompt (exit 0)._

## Docs / infra

- [x] **P4 — verify `cargo doc --workspace --all-features` is warning-clean (after the P0 fix).**
      _Cycle 5 / session-2 cycle-2 (2026-06-05). Verified: now that P0 landed, the full
      `cargo doc --workspace --all-features --no-deps` build completes and emits **0 warnings**
      (`grep -c warning:` → 0), and passes under `RUSTDOCFLAGS="-D warnings"` (exit 0). Made the
      clean state durable so it can't silently regress: added `env: RUSTDOCFLAGS: "-D warnings"` to
      the CI `doc` job (`.github/workflows/ci.yml`) — previously it ran `cargo doc` with no warning
      enforcement — and added a matching `just doc-check` recipe. No Rust code change; code gates
      unaffected. Landed via the cycle PR._
      `cargo doc -p prompt-hub --no-deps` (default features) currently emits **0 warnings**, but the
      full `--all-features` doc build can't complete because it hits the same P0 `audit.rs` compile
      error. Once P0 lands, re-run and drive any feature-gated doc-link/missing-docs warnings to zero.
      _Source: `cargo doc --workspace --all-features --no-deps` (blocked by E0277); TODO.md P2/P4.
      Verify: `cargo doc --workspace --all-features --no-deps 2>&1 | grep -c warning:` → 0._

- [x] **P5 — verify the Docker build and add `.cliff.toml` for Conventional-Commit changelogs.**
      _Cycle 6 / session-2 cycle-3 (2026-06-05). CHANGELOG: added `.cliff.toml` (git-cliff config) —
      the CI `changelog` job already ran `git-cliff --output CHANGELOG.md` but with NO config (silent
      built-in default); now it's tuned: Conventional-Commit grouping (Features/Bug Fixes/CI-CD/Docs/
      Perf/Refactor/Testing/Build/Security/Misc), `chore(deps)`+`chore(release)` skipped, subject-only
      entries (a `(?s)\n.*` preprocessor drops bodies). Generated the initial `CHANGELOG.md` and added
      a `just changelog` recipe. REAL verification (not existence-only): installed git-cliff 2.13.1
      (the version CI installs) and ran it — exit 0, valid TOML, well-grouped output. DOCKER: daemon
      not usable in this sandbox (binary present, `docker info` fails) — a tooling limit, not a human
      wall. The CI `docker` job (`ci.yml`) builds `docker build -f docker/Dockerfile -t prompthub:test`
      + `docker run --rm prompthub:test --help` + the builder target on every push, so it's verified
      by CI on this PR. Local static check passed: all 9 Dockerfile COPY source paths
      (prompt-hub/{migrations,templates,README.md,benches}, etc.) exist in the tree. Landed via the
      cycle PR._
      `docker/Dockerfile` exists; confirm `docker build -f docker/Dockerfile -t prompthub:test .`
      succeeds. No `.cliff.toml` exists at repo root — add one so the changelog generates from the
      Conventional-Commit history, enabling docs-scribe's automated changelog path. `justfile` has a
      `docker` recipe but no changelog recipe.
      _Source: `ls .cliff.toml` → absent; `ls docker/Dockerfile` → present; TODO.md P5._

- [!] **blocked: Regenerate `docs/audits/qodana.sarif.json` — needs Qodana scanner (Docker + `QODANA_TOKEN`), unavailable locally.**
      _Session-3 cycle-1 (2026-06-06): verified this is a genuine tooling wall for the local runner —
      Docker daemon not usable (`docker info` fails), no `qodana` CLI, no `QODANA_TOKEN`. The Qodana
      scanner runs via the JetBrains Docker image, and even **CI skips the scan unless the
      `QODANA_TOKEN` secret is set** (`qodana_code_quality.yml`: `if [ -n "$QODANA_TOKEN" ]`).
      Faking a SARIF is forbidden. Resolution path: set the `QODANA_TOKEN` repo secret and let the CI
      Qodana job regenerate + commit the artifact, OR run `qodana scan` locally once Docker is
      available. Not a loop-halting NEEDS-HUMAN — single blocked item; loop proceeds._
      Generated 2026-06-04 00:11, before PRs #27/#28/#30/#31/#32. Cycle 3 confirmed its 39
      `CargoUnusedDependency` + 21 `NewCrateVersionAvailable` findings are obsolete and its code-smell
      line numbers have drifted (used the compiler as ground truth instead). Re-run the CI Qodana job
      (`.github/workflows`) and commit the fresh SARIF so `scripts/update_todo_from_audit.py` and the
      next DISCOVER triage against accurate data. _Source: cycle-3 triage + `/verify` 2026-06-05._

- [x] **Fix bench compile under `criterion` 0.8: `criterion::black_box` is deprecated (`-D deprecated`).**
      _Cycle 8 / session-3 cycle-2 (2026-06-06). Fixed Rust-native: replaced `criterion::black_box`
      with `std::hint::black_box` in all three benches (`db_write_throughput.rs`,
      `embedding_generation.rs`, `search_latency.rs`) — dropped it from each `use criterion::{…}` and
      added `use std::hint::black_box;` (call sites unchanged). Verified across the boundary:
      `cargo clippy --workspace --all-features --all-targets -- -D warnings` now CLEAN (was 18
      deprecated errors), `cargo bench --workspace --no-run` compiles (1m07s), fmt clean, 675 tests /
      0 fail. Landed via the cycle PR._
      Discovered session-3 cycle-1 while running `cargo clippy --all-targets`. The criterion 0.5→0.8
      bump (dependabot) deprecated `criterion::black_box`; all three benches (`search_latency`,
      `embedding_generation`, `db_write_throughput`) still call it, so `cargo clippy --all-targets`
      and `cargo bench` fail under `-D warnings`/`-D deprecated` (18 errors total). NOT caught by the
      canonical `just lint` gate (it lints default targets, not benches), so the workspace is "green"
      per the gate — but benches don't build. Fix Rust-native: replace `criterion::black_box` with
      `std::hint::black_box` in `prompt-hub/benches/*`. _Source: `cargo clippy -p prompt-hub
      --all-targets` (E deprecated). Verify: `cargo clippy --workspace --all-features --all-targets
      -- -D warnings` clean; `cargo bench --no-run` builds._

---

## ✅ Done (verified against current tree — evidence inline)

- [x] **Wire `otel` Prometheus text exposition** — landed on `origin/main` via **PR #28**
      (commit `987f858`, merged `76bcb78`). `otel = ["dep:prometheus"]` (no protobuf / no
      discontinued OTEL exporter); `metrics.rs:328 prometheus_text()` renders v0.0.4 exposition;
      server route `prometheus_metrics` + test `test_prometheus_text_is_valid_exposition`.
      _Verified: `git log origin/main`, `grep prometheus prompt-hub/src/metrics.rs`, `gh pr list`._
- [x] **P3 — sanitization edge-case tests** (zero-width ZWSP/ZWNJ/ZWJ/BOM/word-joiner, RTL/LTR
      overrides, homoglyph vs mixed-script Latin+Cyrillic, negative cases) — landed via **PR #27**
      area, commit `72de246`. `sanitize.rs` carries `test_all_zero_width_variants_blocked`,
      `test_ltr_and_rtl_overrides_blocked`, `test_multiple_zero_width_all_reported`,
      `test_fullwidth_homoglyph_is_suspicious_not_blocked`, `test_pure_cyrillic_is_warning_only`.
      _Verified: `git show 72de246`, `grep "fn test" prompt-hub/src/sanitize.rs`._
- [x] **P3 — LockManager concurrency tests** — commit `72de246`. `lock.rs` carries
      `test_concurrent_create_lock_same_prompt_unique_tokens` (32 racing agents → unique tokens),
      `test_concurrent_verify_only_holder_succeeds`, `test_concurrent_heartbeat_clamps_to_max`
      (via `std::thread::scope`).
      _Verified: `git show 72de246`, `grep "fn test.*concurrent" prompt-hub/src/lock.rs`._
- [x] **Qodana triage round 1: remove 32 unused deps + fix default-feature build** — **PR #27**,
      commit `fad25a1` (merged 2026-06-04 21:16). This is why the SARIF's 39 `CargoUnusedDependency`
      findings are now stale. _Verified: `git log origin/main`, `gh pr list`._

---

## Pre-existing TODO.md history (superseded / context — see TODO.md for full list)

> TODO.md P0 marks most Wave-9 compilation fixes `[x]` and claims "All known blockers fixed; next
> agent should run `cargo check`." **DISCOVER ran it: a NEW blocker appeared post-merge** (the sha2
> 0.11 bump above), so the P0 item at the top of this backlog supersedes that stale "verify" line.
> Honesty over optimism: the build is currently red, not green.

- [x] P0 Wave-9: 13 known compilation-blocker fixes (models/routes/canary/hub/error/auth) — per TODO.md.
- [x] P1: hub.list / storage.list_prompts / audit-log wiring / wave-6 module decls — per TODO.md.
- [x] P2: `#![forbid(unsafe_code)]` on 49/49 library modules — per TODO.md.
