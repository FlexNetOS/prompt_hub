# Cross-Boundary Verification — prompt_hub

The leading cause of real (not compile-time) bugs is a **boundary mismatch**: two sides each look correct in isolation, but their contract breaks where they connect. Existence checks ("the route exists", "it compiles") do not catch these. The fix is to **read both sides simultaneously** and compare shapes. This file is the prompt_hub-specific adaptation of that principle.

## The boundary table

| Boundary | Left (producer) | Right (consumer) | What to compare |
|----------|-----------------|------------------|-----------------|
| Core API ↔ CLI | `PromptHub`/core fn signature + return type (`prompt-hub/src/*.rs`) | the `prompthub` command calling it (`prompthub/src/commands/`, `main.rs`) | arg types, `Result`/`HubError` handling, the value actually printed/used |
| Core API ↔ HTTP | core method shape (`Result<T, HubError>`) | `prompthub-server` route handler + JSON DTO (`routes.rs`) | response shape vs. DTO; error → HTTP status mapping; pagination wrappers (`{ items, total, page }` vs. bare array) |
| Migration ↔ model | column names/types/order in `prompt-hub/migrations/000N_*.sql` | `models.rs` struct fields + `row_to_*`/`get(idx)` mappings | column count & index alignment, name match, nullability vs. `Option<T>` |
| Feature flag ↔ code | `[features]` entry in `Cargo.toml` (`dep:` / sub-features) | `#[cfg(feature="…")]` sites + the **default build** | does default (`cargo check --workspace`) still compile with the code gated out? are optional deps actually `optional = true`? |
| Trait ↔ dyn use | `async fn in trait` definition (`SearchEngine`, `Hook`, …) | boxed-future variant used behind `Arc<dyn …>` | the `dyn`-safe variant exists and matches; no `async_trait` introduced |
| Role/enum ↔ parsing | `Role` enum incl. `Role::Custom(String)`, `Role::Junie` (`models.rs`) | `parse_role` (`main.rs`, serde_json round-trip — not clap `ValueEnum`) | new variants round-trip through the parser |
| Sanitize/RBAC/audit flow | `hub.rs` operation order (sanitize → authorize → mutate → audit → sync → metrics) | the new operation's call site | the new op follows the same pipeline; metrics/audit not skipped |

## Method (for each boundary the change touches)
1. Open the producer file and the consumer file **at the same time**.
2. Extract the producer's output shape (struct, columns, signature).
3. Extract what the consumer expects (DTO, type param, field access, column index).
4. Compare. Any divergence → report **file:line on both sides** + the fix, and notify both responsible agents.

## prompt_hub-specific gotchas
- **Pagination:** core returns `Paginated<T> { items, total, page, per_page }`. A consumer expecting a bare list is a mismatch — confirm it reads `.items`.
- **Column index drift:** `row_to_prompt` and friends read columns by **numeric index**; adding a column mid-`SELECT` shifts every later index. Verify the index used for any blob/embedding column against the `SELECT` order (this exact class of bug lives near `search.rs` embedding extraction).
- **Default-build breakage:** the most common regression. Feature-gated commands/modules/deps must not be referenced from always-compiled code paths. Always run `cargo check --workspace` (default) *separately* from `--all-features`.
- **Error mapping:** `HubError` variants → server HTTP status must stay consistent; a new variant needs a mapping or it falls through to a generic 500.
- **Guard integrity:** a gate made green by adding `#[allow(...)]` to silence, deleting/`#[ignore]`-ing a test, or relaxing `-D warnings` is a **fail**, not a pass.

## Verdict vocabulary
- `pass` — boundary compared on both sides, gates green, behaves per acceptance.
- `fail` — a concrete mismatch (with both-side file:line) or a red gate.
- `unverified` — could not check (offline, missing component, ambiguous) — reported as-is, **never** rounded up to `pass`.
