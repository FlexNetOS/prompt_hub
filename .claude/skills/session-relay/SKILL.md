---
name: session-relay
description: "Hand off the prompt-loop to a fresh session and resume it with zero context loss. ALWAYS use at a cycle budget (HAND OFF) and when starting/continuing the loop in a new session (RESUME). Triggers: 'hand off', 'checkpoint the loop', 'resume', 'pick up the loop', 'continue in a new session', 'relay'. The committed _workspace/HANDOFF.md is the authoritative resume signal."
---

# Session Relay — Durable Handoff & Resume

A long session rots (context fills, quality drops) and burns tokens. The defense is a **chain of short sessions**, each handing a durable checkpoint to the next. This skill is the seam. Truth lives on disk (`backlog.md` + `loop_state.md` + commits + `HANDOFF.md`), so any restart resumes cold with zero loss.

> **Why disk, not inboxes:** the committed `HANDOFF.md` is THE signal. A self-addressed message does **not** land in your own inbox, and a same-machine successor inherits your identity — so messaging is only an *observable heartbeat*, never the payload. (Verified gotcha.)

## Two entry points

### HAND OFF — at a cycle budget
Invoked by `prompt-loop` when `cycles_this_session >= cycle_budget` (or on a deliberate checkpoint). Steps:
1. **Ensure state is committed.** `backlog.md` + `loop_state.md` reflect reality; the current cycle's work is committed (or its partial state is honestly recorded). Nothing important lives only in context.
2. **Write the checkpoint.** Spawn the **continuity-steward** agent (general-purpose, `model: opus`) → it writes `_workspace/HANDOFF.md` (state + pointers + verify-on-resume baseline).
3. **Commit it.** `git add _workspace/HANDOFF.md _workspace/backlog.md _workspace/loop_state.md && git commit` (area-prefix `chore(loop): handoff cycle N`). The commit is the durable resume point.
4. **Heartbeat (best-effort, optional).** Broadcast a weave/mesh heartbeat `to:"all"` with tag `relay:handoff` (cross-identity observability only — not the payload). Skip silently if the mesh is unavailable.
5. **Best-effort successor (optional).** A one-shot `CronCreate {recurring:false}` or `RemoteTrigger` whose prompt self-describes the resume (`/prompt-loop resume from _workspace/HANDOFF.md`). Note: `durable:true` is **not** honored in this runtime (session-only) — so the committed `HANDOFF.md` remains the real signal; a human or the external runner resumes from it.
6. **Stop.** Do **not** `ScheduleWakeup`. Under the external runner, write exactly one sentinel (`HANDOFF.md` = more work remains) and exit.

### RESUME — starting/continuing in a new session
Invoked by `/prompt-loop resume from _workspace/HANDOFF.md` (or any resume trigger). Steps:
1. **Read the committed `HANDOFF.md`** (authoritative). If absent → this is not a resume; fall back to `prompt-loop` DISCOVER.
2. **Run the verify-on-resume baseline** from the handoff (e.g. `cargo check --workspace`, `just test`, `just lint`, `git status --short`). If it fails, the tree is not sane — repair/triage before continuing; do not build on a red baseline.
3. **Heartbeat** `relay:resumed` `to:"all"` (best-effort).
4. **Reset the per-session counter:** `cycles_this_session = 0` in `loop_state.md` (carry `cycles_total`).
5. **Continue** at the backlog's current item — re-enter the `prompt-loop` iteration.

## Sentinel contract (the external runner reads exactly one per process)
| Sentinel (`_workspace/…`) | Meaning | Runner action |
|---------------------------|---------|---------------|
| `HANDOFF.md` | more work remains | spawn the next fresh process |
| `DONE` | finished + verified (evidence inside) | exit 0 |
| `NEEDS-HUMAN` | human wall (reason inside) | halt for human |
| `STOP` | kill switch (human `touch`es it) | halt |

## Principles
- **Commit before you signal.** A heartbeat or successor that points at uncommitted state is a lie.
- **Honest handoff.** Record blockers (`- [!]`, `NEEDS-HUMAN`) and partial progress truthfully; never imply a cycle finished that didn't.
- **One sentinel per process** under the external runner — writing two (or none) breaks the loop's termination logic.
- **Fail-closed.** If you can't safely checkpoint, write `NEEDS-HUMAN` with the reason rather than a false `HANDOFF`/`DONE`.
