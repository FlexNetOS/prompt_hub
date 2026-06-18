# Cycle 83 — Implementer Notes: wire `tokens.rs` into the `PromptHub` core façade

## Summary
Wired the previously-dead `prompt-hub/src/tokens.rs` module into the core `PromptHub`
façade via two RBAC-gated, async methods that operate on a **stored** prompt. Logic stays
in `tokens.rs`; the hub methods are thin fetch+delegate shells. Core-only — `prompthub-server/`
untouched (avoids the in-flight routes.rs PR).

## Files touched
- `prompt-hub/src/hub.rs` — added two methods (after `get_by_id`, ~line 938) + 4 inline tests.
- `_workspace/cycle_83_implementer_notes.md` — this file.

(No changes to `tokens.rs`, no new migrations, no new dependencies, no feature-gate changes.)

## Methods added (signatures)
```rust
pub async fn count_prompt_tokens(
    &self,
    id: Uuid,
    model: &str,
    identity: &AgentIdentity,
) -> Result<crate::tokens::TokenCount>

pub async fn estimate_prompt_cost(
    &self,
    id: Uuid,
    model: &str,
    expected_output_tokens: usize,
    identity: &AgentIdentity,
) -> Result<crate::tokens::CostEstimateDetail>
```
Both: `#[instrument(skip(self))]`, rustdoc with `# Arguments` / `# Returns` / `# Errors`
matching neighboring hub methods. Types fully-qualified as `crate::tokens::...` (tokens is
not `use`-imported at the top of hub.rs; this matches the file's convention of fully-qualifying
non-imported module types).

## RBAC handling — does `get_by_id` authorize?
**Yes.** `PromptHub::get_by_id` (hub.rs:932) calls `RbacAuthManager::authorize_action(identity,
Action::Read)?` as its first line (verified by reading hub.rs:933 and auth.rs:97 where
`Action::Read => Capability::Read`). Both new methods therefore reuse that single Read gate via
`self.get_by_id(id, identity).await?` — **no second authorize was added** (no double-gate).
`None` is mapped to `HubError::NotFound(id.to_string())`.

## Tests added (4, inline in `hub::tests`)
1. `test_count_prompt_tokens` — register prompt, `count_prompt_tokens(id, "gpt-4", &agent)` →
   asserts `model == "gpt-4"` and `tokens >= 1`.
2. `test_estimate_prompt_cost` — register prompt, `estimate_prompt_cost(id, "gpt-4", 100, &agent)`
   → asserts `model == "gpt-4"`, `input_tokens >= 1`, `output_tokens == 100`, `total_cost >= 0.0`.
3. `test_count_prompt_tokens_not_found` — random uuid on BOTH methods → `HubError::NotFound`.
4. `test_count_prompt_tokens_unauthorized` — identity with `capabilities: vec![]` on BOTH methods
   → `HubError::Unauthorized` (confirms the reused Read gate fires). Constructed via the
   `AgentIdentity` literal already used by the `test_agent()` helper; registration uses a
   separate authorized writer.

Used the existing `test_config()` / `test_agent()` / `test_prompt()` helpers — no new scaffold.

## Gate results (command → result)
| Command | Result |
|---|---|
| `cargo check --workspace --all-features` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS (no issues) |
| `cargo fmt --all` then `--check` | PASS (clean) |
| `cargo build -p prompt-hub --features tiktoken` | PASS |
| `cargo build -p prompt-hub` (default features) | **FAIL — PRE-EXISTING, not my change** |
| `cargo test -p prompt-hub --all-features --lib prompt_tokens` (3 tests) | PASS (3/3) |
| `cargo test -p prompt-hub --all-features --lib estimate_prompt_cost` (hub one) | PASS |

### Pre-existing default-build failure (NOT introduced by this cycle)
`cargo build -p prompt-hub` (default features) fails with:
```
error[E0432]: unresolved import `argon2::password_hash::rand_core::OsRng`
  the item `OsRng` is gated behind the `getrandom` feature
```
This is in `auth.rs` (argon2/rand_core feature plumbing), **unrelated to tokens/hub**. Verified
pre-existing by `git stash` → `cargo build -p prompt-hub` on the clean tree → **same failure**
(EXIT 101) → `git stash pop`. So the default-feature build is red independently of this change;
the authoritative `--all-features` check, all-features clippy, the tiktoken-feature build, and
fmt are all green. Did NOT weaken any gate to mask it — flagging for the curator/verifier as a
separate latent issue.

### Test-hang note
The full `cargo test` is known to hang on this machine (pre-existing). I name-filtered via
`--lib <substr>` with a 300s `timeout` guard; all runs completed in <0.3s, no hang. Note:
`cargo test` accepts only ONE positional TESTNAME substring (passing two args errors), and
`test_estimate_prompt_cost` also exists in `tokens::tests` — I scoped with `--lib` and confirmed
`hub::tests::test_estimate_prompt_cost ... ok` distinctly.

## Follow-ups discovered (for backlog-curator, not this commit)
- Default-feature build of `prompt-hub` is broken (`argon2`/`rand_core` `getrandom` feature not
  enabled under default features). Latent; worth a dedicated fix cycle.
- Optional future wiring: expose `count_prompt_tokens` / `estimate_prompt_cost` via the CLI
  (`prompthub`) and/or server routes (server deferred — routes.rs PR in flight).
