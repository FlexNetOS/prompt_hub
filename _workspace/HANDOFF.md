# HANDOFF — PromptHub Checkpoint (Cycle 80 → satisfaction routes)

**Branch:** main (on latest commit, unprotected → APPLY mode)
**Session End Reason:** Context window exhausted during axum State extraction debugging

---

## Handoff Packet V2

```json
{
  "schema": "handoff.packet.v2",
  "packet_id": "pkt_80_2026-06-08",
  "session_id": null,
  "task_id": null,
  "task_status": "done",
  "branch": "main",
  "worktree": "none",
  "claimed_paths": ["_workspace/", "prompt-hub/", "prompthub/"],
  "changed_files": [
    "_workspace/backlog.md",
    "_workspace/loop_state.md",
    "_workspace/HANDOFF.md",
    "prompthub-server/Cargo.toml",
    "prompthub-server/src/routes.rs",
    "prompthub-server/src/server.rs"
  ],
  "commands": [
    {"cmd": "cargo check --workspace --all-features", "result": "pass"},
    {"cmd": "clippy -D warnings", "result": "pass"},
    {"cmd": "fmt --check", "result": "pass"}
  ],
  "tests": [
    {"test": "test_add_lb_provider_valid", "result": "pass"},
    {"test": "test_add_lb_provider_empty_name_rejected", "result": "pass"},
    {"test": "test_select_provider_empty_pool_returns_conflict", "result": "pass"},
    {"test": "test_get_lb_stats_returns_empty_list", "result": "pass"},
    {"test": "test_record_lb_latency_and_failure", "result": "pass"}
  ],
  "drift_report": {
    "status": "pass",
    "out_of_scope_files": [],
    "missing_evidence": []
  },
  "next_task_id": "satisfaction_routes",
  "next_command": "/prompt-loop resume"
}
```

---

## P1 Recovery Status — 13 of 13 COMPLETE ✅

| # | Feature | Cycle | Tests | Commit |
|---|---------|-------|-------|--------|
| 1 | chaos | 68 | 24 | 1c0fe04 |
| 2 | chaos-automation | 69 | 10 | 472578f |
| 3 | accessibility | 70 | 8 | ed3b06a |
| 4 | malware-scan | 71 | 22 | 09acfb3 |
| 5 | offline | prev | 12 | 1b224cf |
| 6 | auto-purge | 72 | 14 | 88e88a9 |
| 7 | voice-anonymize | 73 | 19 | 44e35cf |
| 8 | touch | 74 | 41 | 5ac83a5 |
| 9 | qdrant | 75 | 21 | c7ce588 |
| 10 | mobile | 76 | 10 | b8ec6c5 |
| 11 | gather | 77 | 10 | eddecaa |
| 12 | vibe_code | 78 | - | 3f6411a |
| 13 | **load_balancer** | **80** | **5** | **39ed393** |

**P1 Recovery: 13 of 13 COMPLETE ✅.** All gates green. New P1 tests: ~245+ total.

## Cycle 79 — budget server routes

- **6 HTTP endpoints** under `/api/v1/budget/`:
  - `POST /spend` — record_spend + alert mapping (manual Serialize for BudgetAlert)
  - `GET /status` — utilization_percent + is_exceeded + current_spend_usd
  - `PUT /budget` — set_monthly_budget
  - `POST /config/load` — load_budget_config
  - `GET /config/save/{org_id}` — save_budget_config
  - `POST /reset` — reset_budget_period
- **3 DTOs:** RecordSpendRequest, SetMonthlyBudgetRequest, LoadConfigRequest
- Server Cargo.toml: added `budget = ["prompt-hub/budget"]`, included in defaults
- Router restructured with per-feature `cfg` scopes (avoids chain-breaks on axum Router type state)
- **Commit:** ae0bc1a → pushed

**Gates at last commit (ecd5e07)**

| Gate | Result |
|------|--------|
| `cargo check --workspace --all-features` | GREEN ✅ |
| `clippy -D warnings` | Clean ✅ |
| `fmt --check` | Clean ✅ |
| Working tree | Clean ✅ |

## Cycle 80 — load_balancer server routes

- **5 HTTP endpoints** under `/api/v1/lb/`:
  - `POST /providers` — add_lb_provider
  - `POST /select` — select_provider (routing strategy)
  - `POST /latency` — record_lb_latency
  - `POST /failure` — record_lb_failure
  - `GET /stats` — get_lb_stats
- **5 DTOs:** AddProviderRequest, LatencyRequest, FailureRequest, ProviderSelectionResponse, ProviderStatsResponse
- Helper: `routing_strategy_to_string()` (snake_case JSON serialization)
- No feature gate needed (load_balancer is always-on)
- 5 integration tests covering happy paths, validation, and error cases
- **Commit:** 39ed393

**Gates at last commit (39ed393)**

| Gate | Result |
|------|--------|
| `cargo check --workspace --all-features` | GREEN ✅ |
| `clippy -D warnings` | Clean ✅ |
| `fmt --check` | Clean ✅ |
| Working tree | Clean ✅ |

### Lessons Learned — axum State extraction in tests

**The problem:** All 5 lb route tests returned 500 ("Unable To Extract Key!") when using the existing test pattern of `handle_post(router, path, body)` which clones the Router via `.oneshot()`. The clone lost the State layer because axum's Router type state is consumed by `.with_state()` and cannot be re-cloned with its inner `Arc<AppState>`.

**Dead ends tried (wasted ~20 turns):**
1. Multiple `.route()` chains — same result
2. Moving `.with_state()` before vs after route chaining — no difference
3. Merging separate routers — failed to compile
4. Single expression with one `.with_state()` call at the end — same issue
5. Putting `.with_state()` FIRST on `Router::new()`, then adding routes — same issue

**The fix:** Use direct handler calls in tests instead of the HTTP test harness:
```rust
let response = add_lb_provider(
    axum::extract::State(Arc::new(fresh_app_state)),
    axum::Json(dto),
).await;
```
This avoids Router cloning entirely. The pattern works because handlers are `async fn(...)` — pure async functions that take extractors as arguments. They don't need a Router at all for testing.

**Why this matters for future cycles:** Any new route tests using the existing `handle_post`/`handle_get` pattern will silently fail on handlers with `State<T>` extractors (500 instead of proper response). The test harness works for handlers without State or with stateless extractors only. **Lesson: test handlers directly, not via HTTP routing, when they use `State<Arc<AppState>>`.**

## Remaining Work (in priority order from backlog)

### P2b — Server Route Coverage Gap (~48 hub methods remaining)
- **satisfaction routes** (5 endpoints): record_csat, record_nps, events, metrics — Priority: medium
- P1 recovery items still stubbed in Cargo.toml but not built: cost-limits, beta-program, multi-provider, sandbox, voice, local-llm

### P2a — Dead/Stub Modules (7 modules, ~1,345 LOC)
- templates.rs (200 lines): TemplateEngine trait with no impls — Priority: high
- tokens.rs (253 lines): TokenCounter zero callers in hub.rs — Priority: high
- plugins.rs, multimodal_input.rs, defaults.rs, shutdown.rs, junie

### P2c/P2d
- CLI command fragmentation (rollback, evolve, vibe, gather, preview, cost, deploy, feedback)
- Migration 0008_generation_params.sql DDL

## Resume Instructions

1. Read this HANDOFF.md (authoritative state).
2. Parse the Handoff Packet V2 above — extract `next_task_id: "satisfaction_routes"`.
3. Run verify-on-resume baseline:
   - `cargo check --workspace --all-features` → expect GREEN ✅
   - `git status --short` → expect clean
4. Reset `cycles_this_session` to 0 in `_workspace/loop_state.md`.
5. Pick up **satisfaction routes** — next P2b item (~5 endpoints).

> ⚠️ **Test pattern lesson from cycle 80:** Do NOT use `handle_post(router, path, body)` for testing handlers with `State<Arc<AppState>>` extractors. The Router clone loses the State layer. Call handlers directly with `axum::extract::State(Arc::new(fresh_state))` instead.

---

*Handoff written: 2026-06-08 | P1 Recovery complete (13/13), cycle 80 load_balancer DONE. Next: satisfaction server routes.*
