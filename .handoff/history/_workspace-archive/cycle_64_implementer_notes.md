# Cycle 64: gradual-rollout — Implementer Notes

## Summary
Replaced stale `canary` feature with full `gradual-rollout` feature. The old `canary` was a passthrough stub (feature flag + empty module file) that provided no value. `gradual-rollout` is a real deployment system.

## Key decisions made during implementation
1. **No separate types module** — all model types go in models.rs alongside CanaryDeployment (follows existing pattern from beta-program/cost-limits)
2. **RolloutEngine is stateless** — follows the CanaryEngine pattern; callers manage config lifecycle via PromptHub struct
3. **canary feature fully removed** — zero references to `"canary"` as a feature string remain
4. **CanaryDeployment kept in models.rs** — used by the backward-compat `canary_deploy()` method

## Pre-existing bugs fixed during this cycle
1. `hub.rs:24` — `use crate::models::CanaryDeployment;` had no cfg gate, would break default-features compile if canary was disabled
2. `hub.rs` — `debug!` import gated behind `budget` feature but used unconditionally

## What to watch for in sandbox cycle
- Sandbox needs syscalls/network isolation which may require `cfg(unix)` or platform detection
- Memory limits need `rlimit` crate — verify it's not a heavy dependency
- Consider whether sandbox execution needs an actual process fork vs in-memory simulation

## Files touched by this cycle
| File | Action |
|------|--------|
| prompt-hub/Cargo.toml | Added `gradual-rollout`, removed `canary` |
| prompthub/Cargo.toml | Wired passthrough: canary -> gradual-rollout |
| prompt-hub/src/lib.rs | Swapped mod gate canary → gradual_rollout |
| prompt-hub/src/models.rs | +4 types (RolloutStage, RolloutSegment, AutoRollbackPolicy, GraduatedRolloutConfig) + test |
| prompt-hub/src/gradual_rollout.rs | NEW — RolloutEngine with 6 tests |
| prompt-hub/src/hub.rs | +5 delegation methods, -2 old canary methods, fixed imports |
| prompt-hub/src/canary.rs | DELETED |

## Verification evidence
- `cargo check --workspace --all-features` ✅ (3 crates compiled)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅ (No issues found)
- `cargo test --workspace --all-features` ✅ (759 passed, 2 ignored)
- `cargo fmt --check` ✅
