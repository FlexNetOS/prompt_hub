#!/usr/bin/env python3
"""Migration generator: _workspace/backlog.md -> .handoff/tasks/*.task.json

One-time, reproducible conversion of prompt_hub's deprecated _workspace backlog
into canonical `handoff.task.v1` cards (meta/handoff schema). Run from repo root:

    python3 .handoff/history/generate_cards.py

Provenance is preserved in .handoff/history/_workspace-archive/. The resume
packet is NOT authored here — it is a derived view; regenerate it with
`hf fleet render prompt_hub` after the cards exist (ADR-0004 §3).

Priority mapping from the prose backlog: compile-critical -> P0, "high" -> P1,
"medium" -> P2, "low" -> P3. Status: shipped -> done, open -> backlog, the P4
default-identity item -> blocked (documented workaround, not actionable now).
"""
import json
import pathlib

TASKS = pathlib.Path(".handoff/tasks")
CORE = ["prompt-hub/**"]
SERVER = ["prompthub-server/**"]
CLI = ["prompthub/**"]
MIGRATIONS = ["prompt-hub/migrations/**"]
GATES = [
    "cargo check --workspace --all-features",
    "cargo clippy --workspace --all-targets --all-features -- -D warnings",
    "cargo fmt --all --check",
]
GATES_TEST = GATES + ["cargo test --workspace --all-features <name>  # full suite hangs on this host; name-filter"]

# (num, title, status, priority, scope, objective, [acceptance], [tests], extra)
S = []
def add(num, title, status, prio, scope, objective, acceptance, tests=None, **extra):
    S.append((num, title, status, prio, scope, objective, acceptance, tests or GATES, extra))

# ---- P0 compile-time fixes (DONE) ----
add(1, "Remove dead `quality = []` Cargo feature stub", "done", "P0", CORE,
    "P0: `quality = []` in Cargo.toml had no backing module -> compile risk. Resolved by removing the stale stub (quality_gate concept already wired via PR #50).",
    ["`quality = []` removed or backed by a module; workspace builds --all-features"], commit="b482efd")
add(2, "Feature-gate rollback pub methods (`#[cfg(feature = \"rollback\")]`)", "done", "P0", CORE,
    "deploy_with_rollback/restore_snapshot/is_rollback_available at hub.rs referenced a rollback-only struct field; without the cfg gate, building without `rollback` compiled these against a non-existent field -> compile failure. Added the gate.",
    ["Building without `rollback` feature compiles clean"], commit="7aa2c4e")
add(3, "Route prompt fetch through hub RBAC (`get_by_id`) in server", "done", "P0", SERVER,
    "Server GET route bypassed intent/audit/RBAC by calling storage().get_prompt() directly. Changed to the hub's RBAC-gated path so all CRUD routes go through hub methods.",
    ["Server GET prompt route calls a hub RBAC method, not storage directly"], commit="2feb13f")

# ---- P1 recovered features (DONE) ----
p1 = [
    (4,  "beta-program",     "P1", "Beta testing program management: beta cohorts, rollout percentages, feedback collection (phased deployment).", "6b78a63", None),
    (5,  "chaos",            "P2", "Chaos engineering for prompt eval: 6 fault-injection strategies, deterministic RNG, severity scoring. 14 tests.", "1c0fe04", 68),
    (6,  "chaos-automation", "P2", "Scheduled chaos tests with trend detection: tokio Interval scheduler, linear-regression trend, alert actions, bounded history. 10 tests.", "472578f", 69),
    (7,  "gradual-rollout",  "P1", "Replaced stale `canary` with full gradual-rollout: RolloutStage/Segment, AutoRollbackPolicy, RolloutEngine (SHA-256 bucket hashing, auto-rollback, stage advance).", "05ad5d2", None),
    (8,  "voice-anonymize",  "P3", "Regex PII detection (email/phone/SSN/CC/IPv4/DOB/custom) + AnonymizerBuilder with overlap protection. Voice transcript scrubber.", "44e35cf", 73),
    (9,  "cost-limits",      "P1", "Multi-dimensional cost enforcement per tenant/agent/project: budget caps, spend alerts, overage blocking, quota-by-resource-type.", "1b05e3d", None),
    (10, "malware-scan",     "P2", "Heuristic malware detection: magic numbers, shellcode, script injection, encoded-payload entropy, extension/content mismatch. 22 tests.", "09acfb3", 71),
    (11, "sandbox",          "P1", "Per-prompt execution sandbox within #![forbid(unsafe_code)]: SandboxMode, SandboxConfig bounds, CRUD engine w/ rate/token/cost/network checks. HubError::SecurityViolation. 15 tests.", "4c01df7", None),
    (12, "multi-provider",   "P1", "Multi-model provider routing: cross-vendor A/B, health-based fallback chains, cost-aware routing. Extends load_balancer to heterogeneous providers.", "6b78a63", None),
    (13, "offline",          "P2", "In-memory OfflineStore mirroring CRUD; pending push/pull queues; 4 conflict strategies (LWW/LocalWins/ServerWins/Merge); local-first eventual sync. 12 tests.", "1b224cf", 72),
    (14, "qdrant",           "P3", "Qdrant REST client (health/ensure/upsert/delete/search), VectorSearchMode, QdrantEngine: SearchEngine impl with hybrid rank fusion. 21 tests.", "c7ce588", 75),
    (15, "local-llm",        "P1", "Local inference integration within #![forbid(unsafe_code)]: LocalModelConfig builder, health enum, Ollama/llamafile client, config CRUD + health checks. No new deps. 13 tests.", "ff05895", None),
    (16, "mobile",           "P3", "NetworkCondition/CellularGeneration bandwidth estimation, SyncStrategy, MobileConfig, pending-push queue, bandwidth-aware sync planning + suppression. 10 tests.", "b8ec6c5", 76),
    (17, "accessibility",    "P2", "WCAG output formatting: PlainText/StructuredJson/DyslexiaFriendly/HighContrastBraille, multi-sensory mode. 8 tests.", "ed3b06a", 70),
    (18, "touch",            "P2", "Touch->gesture mapping (Tap/Swipe/LongPress/Pinch/MultiTap), sensitivity thresholds, haptic types, TouchDispatcher trait. 41 tests.", "5ac83a5", 74),
    (19, "voice",            "P1", "Voice pipeline orchestration (STT->prompt->TTS): VoicePipelineConfig, output format, interaction transcript, FSM, engine state machine. 18 tests + hub integration.", "47f7bb7", None),
    (20, "gather",           "P2", "SmartContextGatherer: priority-ranked file discovery (6 categories, depth decay), code pattern extraction (imports/sigs/structs/traits), SmartContext wrapper. 10 tests.", "eddecaa", 77),
    (21, "auto-purge",       "P2", "Periodic purge daemon (tokio Interval): DaysOld/Tags/Domain/Status policies first-match-wins; Delete/Archive/Retain; atomic archive-then-delete; AtomicUsize stats. 14 tests.", "88e88a9", 72),
]
for num, feat, prio, obj, commit, cycle in p1:
    add(num, f"Recover feature: `{feat}`", "done", prio, CORE,
        f"P1 recovery (feature prematurely removed from Cargo.toml during s11-s15 wiring cleanup, rebuilt). {obj}",
        [f"`{feat}` implemented in prompt-hub core, feature-gated, tests green, gates clean"],
        commit=commit, **({"cycle": cycle} if cycle else {}))

# ---- Server routes + tokens wiring (DONE) ----
add(22, "Server route: vibe_code", "done", "P1", SERVER,
    "POST /api/v1/vibe/code with VibeCodeRequest DTO + parse_skill_level; full JSON response. Added `vibe` feature pass-through in server Cargo.toml.",
    ["POST /api/v1/vibe/code wired to hub; gates green"], commit="3f6411a", cycle=78)
add(23, "Server routes: budget (6 endpoints)", "done", "P1", SERVER,
    "6 endpoints under /api/v1/budget/ (spend/status/budget/config load+save/reset) + 3 DTOs; per-feature cfg router scopes; `budget` feature pass-through.",
    ["6 budget endpoints wired to hub; gates green"], commit="ae0bc1a", cycle=79)
add(24, "Server routes: load_balancer (5 endpoints)", "done", "P2", SERVER,
    "5 endpoints under /api/v1/lb/ (providers/select/latency/failure/stats) + DTOs + routing_strategy_to_string. Lesson: test axum State handlers directly, not via the handle_post Router clone harness.",
    ["5 lb endpoints wired; 5 tests via direct-handler pattern; gates green"], commit="39ed393", cycle=80)
add(25, "Server routes: satisfaction (4 endpoints)", "done", "P2", SERVER,
    "POST /api/v1/satisfaction/{csat,nps,events} + GET /metrics; 4 DTOs, 7 tests (direct-handler pattern).",
    ["4 satisfaction endpoints wired; gates green"], commit="0b29dce", cycle=81)
add(26, "Server route: evolve_prompt", "done", "P1", SERVER,
    "POST /api/v1/prompts/{id}/evolve thin shell over hub.evolve_prompt; EvolvePromptRequest{strategy} + parse_evolution_strategy (6 variants); HubError->HTTP map; 7 direct-handler tests.",
    ["evolve route wired; gates green; PR merged"], commit="a5e3f6d", cycle=82, pr=80)
add(27, "Wire dead `tokens.rs` into PromptHub façade", "done", "P1", CORE,
    "Brought the zero-caller tokens module live: count_prompt_tokens + estimate_prompt_cost, RBAC Read via get_by_id reuse, None->NotFound. Logic stays in tokens.rs. 4 hub tests incl. unauthorized.",
    ["count_prompt_tokens + estimate_prompt_cost on PromptHub; 4 tests; gates green; PR merged"],
    commit="7f6db50", cycle=83, pr=81)

# ---- OPEN: P0 (found cycle 83) ----
add(28, "Fix default-features build of prompt-hub (argon2 OsRng)", "backlog", "P0", CORE,
    "`cargo build -p prompt-hub` with DEFAULT features fails: E0432 unresolved import argon2::password_hash::rand_core::OsRng in auth.rs. Stash-confirmed PRE-EXISTING; masked because CI only builds --all-features. Fix: gate the OsRng import behind the feature that provides getrandom, or add a default-features CI job + fix root cause.",
    ["`cargo build -p prompt-hub` (default features) succeeds", "a default-features build job exists in CI"],
    tests=["cargo build -p prompt-hub"])

# ---- OPEN: P2a dead/stub modules ----
add(29, "Decide `defaults.rs` (seed_database no-op)", "backlog", "P2", CORE,
    "defaults.rs (117 lines): seed_database() is Ok(()) no-op; template constants duplicated. Decision: implement real seeding or remove the pub mod.",
    ["defaults.rs either seeds for real or is removed; no dead pub surface"])
add(30, "Wire or gate `shutdown.rs` (ShutdownCoordinator)", "backlog", "P3", CORE,
    "shutdown.rs (119 lines): ShutdownCoordinator never instantiated outside module; wait_for_signal() incomplete. Wire into PromptHub::shutdown() or mark internal with a proper cfg gate.",
    ["ShutdownCoordinator is wired into a real shutdown path or gated internal-only"])
add(31, "Complete or remove `multimodal_input.rs`", "backlog", "P1", CORE,
    "multimodal_input.rs (345 lines): process() has empty match arms for all InputType variants; zero references. Complete the file-upload handling (extends multimodal PR #53) or remove the pub mod.",
    ["multimodal_input process() handles all InputType variants OR module removed"])
add(32, "Safe plugin discovery for `plugins.rs`", "backlog", "P2", CORE,
    "plugins.rs (306 lines): PluginRegistry has list/register but dynamic loading is disabled by #![forbid(unsafe_code)]. Implement safe inventory-based discovery or gate behind an unsafe-compatible feature.",
    ["plugins.rs offers a safe discovery path or is explicitly feature-gated"])
add(33, "Wire `templates.rs` TemplateEngine (verify stale claim)", "backlog", "P1", CORE,
    "templates.rs (200 lines): TemplateEngine trait — backlog claimed 'no impls', but HandlebarsEngine + TeraEngine impls EXIST (lines 71/113). Verify whether they are wired into the hub rendering path; wire them as the default renderer or remove if truly unused.",
    ["TemplateEngine impls are wired into hub rendering OR the unused trait is removed, with the stale 'no impls' claim corrected"])
add(34, "Make Junie a first-class PromptHub field", "backlog", "P3", CORE,
    "JunieHook only reachable via hooks module; no direct hub field, no dedicated CLI wiring beyond commands/junie.rs. Add Junie as a first-class PromptHub field with accessors.",
    ["PromptHub exposes a Junie accessor; CLI wiring consistent"])

# ---- OPEN: P2b/c/d ----
add(35, "Cover remaining hub methods with server routes", "backlog", "P1", SERVER,
    "~48 hub methods have full core impls but NO HTTP surface (evolve + satisfaction now done). Wire the next top-priority methods (e.g. fallback chains, remaining feature engines) as routes.",
    ["next batch of uncovered hub methods exposed as documented HTTP routes with tests"])
add(36, "Move inline CLI commands to dedicated files", "backlog", "P2", CLI,
    "rollback/evolve/vibe/gather/preview/cost/deploy/feedback are dispatched inline in main.rs with no dedicated command files -> test/maintenance gap. Extract to commands/*.rs.",
    ["the listed inline CLI commands live in dedicated commands/*.rs files with tests"])
add(37, "Write real DDL for migration 0008_generation_params", "backlog", "P2", MIGRATIONS,
    "0008_generation_params.sql is ~all comments (~1 line SQL). Add the generation_params table / ALTER TABLE if not present, or restructure as an application-layer check.",
    ["migration 0008 contains real, applied DDL or is consciously restructured"],
    tests=["cargo test -p prompt-hub migration"])

# ---- OPEN: P3 tests ----
add(38, "Add tests to `hooks.rs` (orchestrator path)", "backlog", "P1", CORE,
    "hooks.rs (Hook trait, HookRegistry, JunieHook) is security-critical orchestrator infra with ZERO test coverage. Add: pre_execute triggers, post_execute result transformation, hook ordering.",
    ["hooks.rs has tests for pre/post execute and ordering"],
    tests=["cargo test -p prompt-hub hooks"])
add(39, "Integration test for hub.get() RBAC+intent flow", "backlog", "P1", CORE,
    "hub.get() has RBAC + intent logic but no dedicated integration test of the full flow (auth check -> storage lookup -> audit trail).",
    ["an integration test verifies get() auth -> lookup -> audit end to end"],
    tests=["cargo test -p prompt-hub get"])

# ---- OPEN: P4 (blocked / documented workaround) ----
add(40, "Default identity lacks Write for non-operator callers", "blocked", "P3", CORE,
    "AgentIdentity::default() (models.rs:139) returns anonymous with empty capabilities. Server default_agent() grants Read+Write so the HTTP API is fine; this only affects programmatic PromptHub::new() callers. Documented workaround: AgentIdentity::local_operator(). Blocked: design decision needed on whether default should carry Write.",
    ["a decision is recorded on default identity capabilities (and applied or explicitly deferred)"],
    block_reason="Design decision pending; documented workaround exists (local_operator). Not actionable without owner direction on default capability policy.")

# ---- compute hf-identical blake3 intent_locks (drift-sentinel anchors) ----
# Matches work-order::compute_intent_lock: b3(objective), b3(path_scope.join("\n")),
# b3(acceptance.join("\n")), each prefixed "blake3:". Hashing is delegated to the
# tiny b3hash helper so the locks are byte-identical to what `hf claim`/`hf drift`
# would recompute (verified against HFTASK-0001).
import subprocess
B3 = "/tmp/b3hash/target/release/b3hash"
records = []
for num, title, status, prio, scope, objective, acceptance, tests, extra in S:
    records.append(objective)
    records.append("\n".join(scope))
    records.append("\n".join(acceptance))
payload = b"".join(r.encode() + b"\0" for r in records)
out = subprocess.run([B3], input=payload, capture_output=True, check=True).stdout.decode().split("\n")
hashes = [h for h in out if h]
assert len(hashes) == len(records), f"hash count {len(hashes)} != {len(records)}"

# ---- emit ----
TASKS.mkdir(parents=True, exist_ok=True)
count = 0
for i, (num, title, status, prio, scope, objective, acceptance, tests, extra) in enumerate(S):
    tid = f"PHTASK-{num:04d}"
    card = {
        "schema": "handoff.task.v1",
        "id": tid,
        "title": title,
        "status": status,
        "priority": prio,
        "objective": objective,
        "path_scope": scope,
        "acceptance_criteria": acceptance,
        "test_commands": tests,
        "dependencies": [],
        "blocked_by": [],
        "allows_network": False,
        "allows_dependency_addition": False,
        "correlation_id": "prompt-hub-construction",
        "role": "implementer",
        "intent_lock": {
            "objective_hash": hashes[i * 3],
            "path_scope_hash": hashes[i * 3 + 1],
            "acceptance_hash": hashes[i * 3 + 2],
        },
        "source": "migrated from _workspace/backlog.md (2026-06-13)",
    }
    card.update(extra)
    (TASKS / f"{tid}.task.json").write_text(json.dumps(card, indent=2) + "\n")
    count += 1
print(f"wrote {count} cards to {TASKS}")
done = sum(1 for s in S if s[2] == "done")
print(f"  done={done} backlog={sum(1 for s in S if s[2]=='backlog')} blocked={sum(1 for s in S if s[2]=='blocked')}")
