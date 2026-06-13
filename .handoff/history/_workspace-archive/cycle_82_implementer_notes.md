# Cycle 82 — Implementer Notes: evolve_prompt server route (P2b)

## Summary
Wired the existing core `PromptHub::evolve_prompt` to a new HTTP endpoint. Thin shell —
zero business logic added to the server; the route parses input, delegates to the hub,
maps `HubError` to HTTP status, serializes the evolved `Prompt`.

## Endpoint
`POST /api/v1/prompts/{id}/evolve`

- Path param: `{id}` — Uuid of the base prompt.
- Body (`EvolvePromptRequest`): `{ "strategy": "<snake_case>" }`, defaults to `"mutate"` when omitted.
- 200 → evolved `Prompt` as JSON (same field shape as `get_prompt`).
- Handler signature:
  ```rust
  pub async fn evolve_prompt(
      State(state): State<Arc<AppState>>,
      Path(id): Path<String>,
      Json(payload): Json<EvolvePromptRequest>,
  ) -> Response
  ```

## Feature gating
None. `evolve_prompt` is always-on in core (no `cfg`/feature gate on the method), so the route
was registered in the unconditional prompt-CRUD block of `create_router` and the handler is
not feature-gated. No `prompthub-server/Cargo.toml` change required.

## DTO + strategy parsing
- `EvolvePromptRequest { strategy: String }` (serde default `"mutate"`).
- `parse_evolution_strategy(&str) -> Result<EvolutionStrategy, String>` — mirrors the
  snake_case style of `parse_skill_level` / `routing_strategy_to_string`. Case-insensitive,
  trimmed. Unknown input returns the offending (normalized) value as `Err` → 400.
- All 6 `EvolutionStrategy` variants handled: `mutate`, `crossover`, `ab_test`, `semantic`,
  `compress`, `expand`.

## Error mapping (matches existing routes.rs conventions)
- Invalid UUID → 400 `BAD_REQUEST` (same as `get_prompt`).
- Unknown strategy → 400 `BAD_REQUEST` with the list of valid names.
- `HubError::NotFound` → 404 `NOT_FOUND`.
- `HubError::Unauthorized` → 403 `FORBIDDEN`.
- any other `HubError` (incl. `Internal("No crossover candidates")`) → 500 `INTERNAL_SERVER_ERROR`.

Added `use prompt_hub::HubError;` (not re-exported via `models::*`).

## Files touched
- `prompthub-server/src/routes.rs` — DTO `EvolvePromptRequest` + `default_evolution_strategy`,
  `parse_evolution_strategy` helper, `evolve_prompt` handler, `HubError` import, 7 tests.
- `prompthub-server/src/server.rs` — registered
  `.route("/api/v1/prompts/{id}/evolve", post(routes::evolve_prompt))` in the always-on CRUD block.

## Tests added (7, all passing)
Direct-handler calls (per cycle-80 lesson — NOT the `handle_post` Router harness, which drops
the `State` layer). Fresh `:memory:` AppState via `evolve_test_state`; `seed_prompt` registers
a base prompt through `hub.register`.

1. `parse_evolution_strategy_covers_all_variants` — unit: all 6 variants + case-insensitive + unknown.
2. `test_evolve_prompt_mutate_happy_path` — seeded prompt, Mutate → 200.
3. `test_evolve_prompt_semantic_strategy` — seeded prompt, Semantic → 200.
4. `test_evolve_prompt_not_found` — random uuid → 404.
5. `test_evolve_prompt_unknown_strategy_rejected` — "teleport" → 400.
6. `test_evolve_prompt_invalid_uuid_rejected` — "not-a-uuid" → 400.
7. `test_evolve_prompt_crossover_empty_pool_errors` — crossover reaches the hub; asserts
   200 or 500 (with only the base present, `list_prompts` returns the base itself as a
   crossover candidate, so it succeeds rather than erroring — documented in the test).

## Gate results
| Command | Result |
|---------|--------|
| `cargo check --workspace --all-features` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS (No issues found) |
| `cargo fmt --all` + `--check` | PASS (clean) |
| `cargo build -p prompthub-server` (default features) | PASS |
| `cargo test -p prompthub-server --all-features evolve` | PASS (6/6) |
| `cargo test -p prompthub-server --all-features parse_evolution_strategy` | PASS (1/1) |

Targeted test invocations did NOT hang (the known full-`cargo test` hang was avoided by
name-filtering; both runs completed in <3s including compile).

## Follow-ups discovered (for backlog-curator, not this cycle)
- Crossover currently can pick the base prompt itself as its own crossover parent when it is
  the only prompt present (core behavior in `hub.rs:1508`). Not a route bug; a possible core
  refinement (exclude `id` from crossover candidates) if a true empty-pool 500 is desired.
- HTTP routes still use a fresh `default_agent()` (always Read+Write) rather than real
  request-scoped RBAC — consistent with all other mutating routes; out of scope here.
