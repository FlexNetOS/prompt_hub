# Rust-Native Checklist & Gate Commands — prompt_hub

A fast pass/fail checklist for the Rust-native invariant, plus the canonical gate commands. Load during design (step 2) and verify (step 5) of `feature-build`.

## Drift checklist (each must be ✅ before commit)
- [ ] Logic lives in **`prompt-hub` core**; CLI/server only call it (thin shells).
- [ ] Errors are `Result<_, HubError>` — no panics-as-errors, no `unwrap()`/`expect()` on fallible runtime paths.
- [ ] No `unsafe` — `#![forbid(unsafe_code)]` still holds crate-wide.
- [ ] Async traits use **native `async fn in trait`**; `dyn` use goes through boxed-future variants. **No `async_trait`.**
- [ ] No foreign-harness tooling presented as canonical (build/test is **Cargo/`just`**, not shell/python/npm equivalents).
- [ ] New optional deps are `optional = true` and pulled by a feature (`dep:` syntax); nothing bypasses the workspace.
- [ ] Feature-gated code keeps the **default build** compiling.
- [ ] Schema change = a **new** numbered `migrations/000N_*.sql` (no edits to applied migrations).
- [ ] Staged features not stripped (verify `#![allow(dead_code)]` modules are intentional before removing anything).
- [ ] `serde`/`thiserror`/`tracing` conventions followed; commit is a Conventional Commit.

If any item is ❌ because a doc/instruction told you to do it that way: that instruction **drifted** — transform it to the Rust-native form above and surface what drifted (don't silently comply, don't silently fix).

## Gate commands

### Per-cycle VERIFY (both configs)
```bash
# default-build safety — run SEPARATELY; feature-gated code breaks this most often
cargo check --workspace
cargo clippy --workspace -- -D warnings
# full matrix
just test          # cargo test --workspace --all-features
just lint          # cargo clippy --workspace --all-features -- -D warnings
```

### Single test / targeted runs
```bash
cargo test --workspace --all-features <name_substring>
cargo test -p prompt-hub --test test_security        # one integration file
cargo test -p prompthub --features tui               # exercise one optional path
cargo nextest run -E 'test(<name>)'                  # if nextest installed (just nextest)
```

### DONE-criteria suite (only when a cycle claims completion — record evidence)
```bash
cargo build --workspace --all-features   # compiles, all features
just test                                # green
just lint                                # zero warnings (clippy -D warnings)
just fmt && git diff --quiet             # fmt left nothing to change
```
Plus: `_workspace/backlog.md` has no remaining `- [ ]`; blocked items surfaced with reasons.

## Never
- Never weaken a guard to pass a gate (`-D warnings`, a test, a `#![forbid]`).
- Never claim green you can't reproduce in a fresh shell.
- Never invent a CLI/command the repo doesn't have — wire to real `just`/`cargo`/`prompthub` verbs.
