# Codemap — prompt_hub "load a prompt" flow

**Question:** Map the *exact* end-to-end flow by which prompt_hub loads (reads/retrieves) a prompt,
across every entry surface, with adversarially-verified evidence.

**Target root:** /home/drdave/Desktop/meta/prompt_hub  ·  Rust 2024 workspace (3 crates)
**Index:** git-kb code, 3429 symbols / 19006 call sites / 113 routes (built this session).

## Entry points (external surface → façade)
| Surface | Site | Façade call |
|---------|------|-------------|
| CLI `get` | prompthub/src/main.rs:62 | `hub.get(role,intent,id)` (search) |
| CLI raw | prompthub/src/main.rs:212 | `hub.storage().get_prompt(uuid)` (RBAC-bypass) |
| HTTP GET /api/v1/prompts/:id | routes.rs:295,309 | `hub.get_by_id(uuid,…)` |
| HTTP POST /api/v1/prompts/get | routes.rs:2547,2567 (server.rs:41) | `hub.get(role,intent,…)` |
| HTTP POST …/render | routes.rs:2094,2109 (server.rs:68) | `hub.render_prompt(uuid,vars,…)` |

## Call graph (verified via git-kb code)
- `get_by_id` (hub.rs:1013) ← render_prompt(1432), count_prompt_tokens(1045), estimate_prompt_cost(1087), routes::get_prompt(309)
- `get_prompt` storage (storage.rs:374) ← get_by_id(1016), transfer_ownership(1322/1325), update(1701/1704), rollback(1754)
- `get` (hub.rs:981) → authorize_action(auth.rs:95 @987), record_request(@988), **PromptHub::search(hub.rs:1117 @995)**
- `render_prompt` (hub.rs:1426) → get_by_id(1432), templates::default_engine(templates.rs:191 @1452), engine.render(@1453)
- FastEngine::search (search.rs:128) → build_fts_query, storage.acquire(154), FTS5 SELECT+JOIN (157-178)

## Seams
- libsql/SQLite single shared migrated conn, WAL; semaphore pool `acquire()` storage.rs:146.
- `prompts_fts` FTS5 virtual table joined by rowid (search path only).
- `row_to_prompt` storage.rs:1550 hydration (uuid/semver/serde-json/chrono).
- SearchEngine trait object-safe via boxed futures (search.rs:128) — Fast/Smart/Hybrid swap.
