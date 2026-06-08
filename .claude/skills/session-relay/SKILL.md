---
name: session-relay
description: "Hand off the prompt-loop to a fresh session and resume it with zero context loss. ALWAYS use at a cycle budget (HAND OFF) and when starting/continuing the loop in a new session (RESUME). Triggers: 'hand off', 'checkpoint the loop', 'resume', 'pick up the loop', 'continue in a new session', 'relay'. The committed _workspace/HANDOFF.md is the authoritative resume signal. End-of-loop handoff produces a Handoff Packet V2 per `.claude/skills/prompt-loop/handoff/schemas/packet.schema.json`."
---

# Session Relay — Durable Handoff & Resume with Handoff Packet V2

A long session rots (context fills, quality drops) and burns tokens. The defense is a **chain of short sessions**, each handing a durable checkpoint to the next. This skill is the seam. Truth lives on disk (`backlog.md` + `loop_state.md` + commits + `HANDOFF.md`), so any restart resumes cold with zero loss.

> **Why disk, not inboxes:** the committed `HANDOFF.md` is THE signal. A self-addressed message does **not** land in your own inbox, and a same-machine successor inherits your identity — so messaging is only an *observable heartbeat*, never the payload. (Verified gotcha.)

## End-of-Loop Handoff Packet (HandoffLedger V2)

At each cycle boundary, session-relay emits a **handoff packet** structured per `.claude/skills/prompt-loop/handoff/schemas/packet.schema.json`. The packet is NOT the source of truth — it is a compiled projection from Git state, backlog, and gate results.

### Handoff Packet structure (v2)

```
{
  "schema": "handoff.packet.v2",
  "packet_id": "pkt_<cycle>_<timestamp>",
  "session_id": "<loop_session_id>",
  "task_id": "<backlog_item_id_or_NULL>",
  "task_status": "done|blocked|in_progress",
  "branch": "<feature_branch>",
  "claimed_paths": ["_workspace/", "prompt-hub/", "prompthub/"],
  "changed_files": ["<list of git diff --name-only entries>"],
  "commands": [
    {"cmd": "cargo check --workspace --all-features", "result": "pass|fail"},
    {"cmd": "just test", "result": "pass|fail"},
    {"cmd": "just lint", "result": "pass|fail"}
  ],
  "tests": [
    {"suite": "prompt-hub::hub", "passed": N, "failed": M},
    {"suite": "prompthub", "passed": N, "failed": M}
  ],
  "drift_report": {
    "status": "pass|soft_fail|hard_fail",
    "out_of_scope_files": [],
    "missing_evidence": []
  },
  "next_task_id": "<next backlog item id or NULL>",
  "next_command": "/prompt-loop resume from _workspace/HANDOFF.md"
}
```

### Session Event emission (session event v1)

At each handoff boundary, emit a `handoff.session_event.v1` session event:

| Event Type | When | Payload |
|------------|------|---------|
| `session_started` | Loop begins | cycle_budget, apply_mode |
| `checkpoint_created` | Handoff packet written | packet_id, task_status |
| `drift_audited` | Drift check runs | drift_report.status |
| `handoff_created` | HANDOFF.md committed | packet_id, git_sha |
| `session_stopped` | Budget reached / DONE | sentinel type |

## Two entry points

### HAND OFF — at a cycle budget
Invoked by `prompt-loop` when `cycles_this_session >= cycle_budget` (or on a deliberate checkpoint). Steps:

1. **Ensure state is committed.** `backlog.md` + `loop_state.md` reflect reality; the current cycle's work is committed (or its partial state is honestly recorded). Nothing important lives only in context.

2. **Emit session events.** Record `session_stopped` event with sentinel type (HANDOFF/DONE/NEEDS-HUMAN).

3. **Compile the handoff packet.** Read current Git state, gate results, and backlog → produce a Handoff Packet V2 object.

4. **Write the checkpoint.** Spawn the **continuity-steward** agent (general-purpose, `model: opus`) → it writes `_workspace/HANDOFF.md` containing the handoff packet JSON (as markdown block) + state + pointers + verify-on-resume baseline.

5. **Commit it.** `git add _workspace/HANDOFF.md _workspace/backlog.md _workspace/loop_state.md && git commit` (area-prefix `chore(loop): handoff cycle N`). The commit is the durable resume point.

6. **Heartbeat (best-effort, optional).** Broadcast a weave/mesh heartbeat `to:"all"` with tag `relay:handoff` (cross-identity observability only — not the payload). Skip silently if the mesh is unavailable.

7. **Best-effort successor (optional).** A one-shot `CronCreate {recurring:false}` or `RemoteTrigger` whose prompt self-describes the resume (`/prompt-loop resume from _workspace/HANDOFF.md`). Note: `durable:true` is **not** honored in this runtime (session-only) — so the committed `HANDOFF.md` remains the real signal; a human or the external runner resumes from it.

8. **Stop.** Do **not** `ScheduleWakeup`. Under the external runner, write exactly one sentinel (`HANDOFF.md` = more work remains) and exit.

### RESUME — starting/continuing in a new session
Invoked by `/prompt-loop resume from _workspace/HANDOFF.md` (or any resume trigger). Steps:

1. **Read the committed `HANDOFF.md`** (authoritative). If absent → this is not a resume; fall back to `prompt-loop` DISCOVER.

2. **Parse the handoff packet** (Handoff Packet V2). Extract: next_task_id, drift_report.status, last gate results.

3. **Run the verify-on-resume baseline** from the handoff (e.g. `cargo check --workspace`, `just test`, `just lint`, `git status --short`). If it fails, the tree is not sane — repair/triage before continuing; do not build on a red baseline.

4. **Emit session event.** Record `session_resumed` event with resume source (HANDOFF.md git SHA).

5. **Heartbeat** `relay:resumed` `to:"all"` (best-effort).

6. **Reset the per-session counter:** `cycles_this_session = 0` in `loop_state.md` (carry `cycles_total`).

7. **Continue** at the backlog's current item — re-enter the `prompt-loop` iteration.

## Sentinel contract (the external runner reads exactly one per process)
| Sentinel (`_workspace/…`) | Meaning | Runner action |
|---------------------------|---------|---------------|
| `HANDOFF.md` | more work remains | spawn the next fresh process |
| `DONE` | finished + verified (evidence inside) | exit 0 |
| `NEEDS-HUMAN` | human wall (reason inside) | halt for human |
| `STOP` | kill switch (human `touch`es it) | halt |

## Policy gates (from handoff/policies/rules.toml)
- **require_checkpoint**: Every handoff MUST include a compiled packet.
- **require_test_evidence**: At least one gate result must show "pass".
- **require_drift_audit**: Drift report status is always recorded; hard_fail blocks handoff.
- **require_next_command**: next_command field is non-null (except in DONE).
- **default_write_mode**: deny_without_claim — agents should only write within claimed paths.

## Principles
- **Commit before you signal.** A heartbeat or successor that points at uncommitted state is a lie.
- **Honest handoff.** Record blockers (`- [!]`, `NEEDS-HUMAN`) and partial progress truthfully; never imply a cycle finished that didn't.
- **One sentinel per process** under the external runner — writing two (or none) breaks the loop's termination logic.
- **Fail-closed.** If you can't safely checkpoint, write `NEEDS-HUMAN` with the reason rather than a false `HANDOFF`/`DONE`.
- **Packet-first handoff.** The compiled Handoff Packet V2 is the operational state; HANDOFF.md wraps it for human readability.
