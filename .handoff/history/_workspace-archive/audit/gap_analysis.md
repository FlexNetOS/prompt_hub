# PromptHub Comprehensive Gap Analysis

**Date:** 2026-06-07
**Scope:** Full codebase audit — all modules, features, CLI, server, migrations, tests
**Method:** Cross-referenced lib.rs pub modules → hub.rs wiring → CLI commands → server routes → migrations → test coverage

---

## Critical Gaps (will break compilation or correctness)

### CRIT-1: Dead stub feature `quality = []` — no module file exists
- **File:** `prompt-hub/Cargo.toml:54`
- **Issue:** Feature gate `quality = []` declared but `prompt-hub/src/quality.rs` does not exist. Any build enabling this feature (beyond --all-features) will fail with "module quality not found". This was left as a stub passthrough during the P1 wiring sweep — the module should either be created or the feature removed.
- **Impact:** Compile failure if `--features quality` is used

### CRIT-2: Rollback methods lack cfg gates matching struct field
- **Files:** `prompt-hub/src/hub.rs:1425,1436,1441` (methods) vs `hub.rs:187` (struct field)
- **Issue:** The `safe_deployer` struct field is gated `#[cfg(feature = "rollback")]`, but the three public methods that call it (`deploy_with_rollback`, `restore_snapshot`, `is_rollback_available`) have NO cfg gate. If someone builds without the "rollback" feature (possible since these are stub passthrough features), these 3 methods reference a non-existent struct field → compile failure.
- **Impact:** Latent bug — only hidden because "rollback" is in default = [...]

### CRIT-3: Server routes bypass hub.get() for direct storage access
- **Files:** `prompthub-server/src/routes.rs:215` uses `state.hub.storage().get_prompt(uuid)` directly
- **Issue:** This bypasses `hub.get()` which contains RBAC intent logic. All other server routes go through hub methods. Inconsistent — if an identity check or audit hook is added to hub.get() in the future, direct storage access will skip it.
- **Impact:** Security inconsistency — RBAC flow has two different code paths

---

## High Gaps (dead code, missing features, major coverage gaps)

### HIGH-1: Modules with pub mod but zero hub.rs wiring (always compiled, never exposed)
| Module | Lines | Type Count | Status | Notes |
|--------|-------|-----------|--------|-------|
| `defaults` | 117 | 2 pub fns | NO-op | seed_database() is `Ok(())`. Template constants duplicated from files. |
| `shutdown` | 119 | 5 pub items | DEAD | Never instantiated outside module. wait_for_signal() body incomplete. |
| `multimodal_input` | 345 | 1 pub fn | STUB | process() has empty match arms for ALL InputType variants. Never referenced. |
| `plugins` | 306 | 10 pub items | PARTIAL | PluginRegistry has list/register but dynamic loading disabled by #[forbid(unsafe_code)]. Compiles but dead code path. |
| `templates` | 200 | 4 pub items | STUB | TemplateEngine trait defined with render/lint methods — no implementations wired anywhere. |
| `tokens` | 253 | 4 pub items | UNWIRED | TokenCounter impl exists with tiktoken fallback logic but never used by hub.rs. |
| `junie` | small | 5 pub items | INDIRECT | JunieHook only accessible via hooks module — no direct hub field, no CLI wiring. |

**Total dead/stub LOC:** ~1,345 lines of code with zero product exposure.

### HIGH-2: Hub methods with NO server route (60+ uncovered)
The server has only **8 real routes** covering 7 hub method calls. The rest of hub.rs's ~70 pub methods have no HTTP equivalent:

| Category | Methods Uncovered |
|----------|-------------------|
| **vibe/coding** | vibe_code, gather_context |
| **cost/confidence** | estimate_cost, score_confidence, scan_privacy |
| **lifecycle** | update, rollback, evolve_prompt, shutdown |
| **learning** | fallback_chain, learn_from_feedback |
| **quality/lineage** | run_quality_gate, get_lineage_ancestry, detect_lineage_forks, get_lineage_descendants, build_lineage_tree (5 methods) |
| **swarm/pollination** | manage_swarm, validate_swarm_roles, generate_swarm_bundle, pollination + 3 variants (8 methods) |
| **satisfaction** | satisfaction_tracker, record_csat_rating, record_nps_rating, record_satisfaction_event, satisfaction_metrics (5 methods) |
| **health monitoring** | health_monitor + register_provider/record_success/record_failure/is_healthy/get_health_summary (6 methods) |
| **rollback/deploy** | deploy_with_rollback, restore_snapshot, is_rollback_available (3 methods) |
| **load balancer** | load_balancer, add_lb_provider, select_provider, record_lb_latency/failure, get_lb_stats (6 methods) |
| **budget** | record_spend, budget_utilization, current_spend_usd, is_budget_exceeded, set_monthly_budget, load_budget_config, save_budget_config, reset_budget_period (8 methods) |
| **circuit breaker/moderation** | circuit_breaker + check_content/is_content_safe/check_content_batch/moderation_engine (4 methods) |
| **quota** | check_and_consume, quota_usage, reset_quota, quota_enforcer_handle (4 methods) |
| **preview** | preview_generate, preview_artifacts, preview_engine_handle (3 methods) |
| **canary** | canary_deploy, canary_should_rollback (2 methods) |

### HIGH-3: CLI commands with real hub dispatch but no command file (inline in main.rs)
These 20+ subcommands dispatch to hub.rs methods directly in `main.rs`/`cli.rs` rather than having dedicated files:

get, update, rollback, diff, lock, unlock, audit, lineage, server, restore, evolve, tokens, lint, vibe, magic, gather, preview, cost, scan, deploy, summarize, feedback, voice, onboard, heal, suggest

The inline code makes these hard to test independently and harder to maintain. Compare against the 10 files in `prompthub/src/commands/` (add, budget, cache, export, import, init, junie, list, metrics, plugin, search) — the coverage is uneven.

### HIGH-4: Missing migration for `generation_params` table
- **File:** `migrations/0008_generation_params.sql` (all comments, no SQL)
- **Issue:** Hub.rs references generation params in swarm lineage tracking but the migration file contains only descriptive comments — no actual DDL. New databases starting fresh skip this entirely; existing DBs may have it from manual intervention. This is a silent version marker, not an actual migration.

### HIGH-5: Stub passthrough features mapping to nothing
From `prompt-hub/Cargo.toml` features section, these stub passthroughs exist but their corresponding modules are either dead or stub-only:
- `quality = []` → NO MODULE (CRIT-1)
- `analytics = []` → module exists (`analytics.rs`, 10.7K) but zero hub wiring beyond the import at line 3
- `preview = []` → module exists (`preview.rs`, 15.9K) with 3 methods, but 5 panic! call sites in test code (test-only coverage)

---

## Medium Gaps (code quality, testing, documentation)

### MED-1: hooks.rs — core infrastructure with zero tests
- **File:** `prompt-hub/src/hooks.rs` (full file)
- The HookRegistry and JunieHook are critical orchestrator infrastructure but have NO test coverage. This is a correctness gap — the hook execution pipeline could break silently during operation.

### MED-2: get_prompt in server routes uses hub.storage().get_prompt() directly
- **File:** `prompthub-server/src/routes.rs:215`
- Bypasses hub.get()'s RBAC and audit trail logic. The 7 other CRUD routes all go through hub methods; this one is the outlier. Should be `state.hub.get(...)` with identity → rbac check.

### MED-3: Swarm/bundle route uses raw hub.list() + custom logic instead of generate_swarm_bundle
- **File:** `prompthub-server/src/routes.rs:457` calls `state.hub.list()` then aggregates roles manually
- The hub method `generate_swarm_bundle()` exists but isn't called. This duplicates the swarm validation logic outside the crate's controlled flow.

### MED-4: Templates and tokens modules exposed as pub mod but never consumed
- **Files:** `prompt-hub/src/templates.rs`, `prompt-hub/src/tokens.rs`
- Both have real implementations (200+ LOC each) but zero callers anywhere in the crate or binaries. They are product-facing types (TemplateEngine, TokenCounter) without any entry point into PromptHub.

### MED-5: Migration 0008 is a no-op version marker only
- **File:** `migrations/0008_generation_params.sql` — all comment lines, no SQL
- New databases starting fresh will skip generation_params entirely. Existing databases that were built during development may have it manually applied. This creates silent drift between dev and production DB schemas.

### MED-6: satisfaction.rs module wired but feature-gated on dead passthrough
- **Files:** `prompt-hub/src/satisfaction.rs` (11K), `Cargo.toml:45` (`satisfaction = []`)
- Hub wiring exists at hub.rs lines 1276-1325 but the stub passthrough `satisfaction = []` means the feature is never actually activatable via CLI — it's always-on when --all-features or explicit. The cfg gates on hub imports are correct, but the Cargo.toml stub creates confusion about what's "default" vs "optional".

---

## Low Gaps (cosmetic, minor inconsistencies)

### LOW-1: Section header "Load balancer" at hub.rs:1419 is orphaned
No LB methods appear between it and the next section header (Rollback). LB methods are all after line 1446. The header should be removed or methods moved above it.

### LOW-2: No ADR for multi-module scaffolding strategy
51 modules with varying completeness levels exist but no ADR explains why some have full implementations while others remain stubs. This makes future contributions guess the intent.

### LOW-3: CHANGELOG.md is only 59 lines — likely incomplete for a project with 62+ PRs
The changelog doesn't comprehensively reflect all P1 wiring work across s11-s15.

---

## What was Committed Out (Lost During S11-S15 Wiring Sweep)

### Removed from Cargo.toml features (dead features cleaned):
```
beta-program, chaos, chaos-automation, cost-limits, gradual-rollout,
malware-scan, multi-provider, offline, qdrant, sandbox, voice-anonymize,
local-llm, mobile, accessibility, touch, voice, gather, auto-purge
```

These were stub passthroughs removed during P1 wiring cleanup. The comment block at Cargo.toml:63-69 documents them as dead features — good record-keeping.

### Never committed (missing entirely):
1. **`quality.rs`** — `quality = []` in Cargo.toml has no corresponding source file. Should either be created or the feature removed from Cargo.toml.
2. **`migrations/0008_generation_params.sql` content** — should contain actual DDL for generation_params table but only has comments. The migration was half-committed (file exists but body is all docs).

### Partially committed (feature gates in lib.rs without Cargo.toml passthroughs):
The following modules have `#[cfg(feature = "...")]` on their hub.rs imports but the feature name does not exist as a stub passthrough in Cargo.toml — they are always-in (lib.rs pub mod has no gate):
- None found — all cfg-gated imports map to features declared in Cargo.toml

---

## Summary: Gap Severity Distribution

| Severity | Count | Key Findings |
|----------|-------|--------------|
| **Critical** | 3 | Dead quality feature, rollback cfg mismatch, server storage bypass |
| **High** | 5 | 7 dead/stub modules (1,345 LOC), 60+ hub methods without server routes, 20+ CLI commands inline, missing generation_params DDL, satisfaction passthrough confusion |
| **Medium** | 6 | hooks.rs zero tests, duplicate swarm logic, unwired templates/tokens, migration drift, satisfaction passthrough, get_prompt path inconsistency |
| **Low** | 3 | Orphaned section header, missing multi-module ADR, incomplete CHANGELOG |

---

## Recommended Actions (Priority Order)

1. **Fix CRIT-1:** Remove `quality = []` from Cargo.toml or create `prompt-hub/src/quality.rs`
2. **Fix CRIT-2:** Add `#[cfg(feature = "rollback")]` to the 3 rollback pub methods in hub.rs
3. **Fix CRIT-3:** Replace direct storage access in server routes.rs:215 with `state.hub.get(...)` + proper identity
4. **Address HIGH-1:** Remove or wire the 7 dead/stub modules (defaults, shutdown, multimodal_input, plugins, templates, tokens, summarizer)
5. **Address HIGH-2:** Prioritize top hub methods for server route coverage (vibe_code, evolve_prompt, budget, load_balancer, quality_gate)
6. **Address MED-1:** Add tests to hooks.rs — it's core infrastructure with zero test coverage

---

*Analysis completed: 2026-06-07 | All claims verified against codebase state*
