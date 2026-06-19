# `_workspace/` — durable loop state

This directory is the construction crew's **memory on disk**. The loop is designed so that any
session (or a fresh process spawned by the runner) can resume from these files alone, with zero
context loss. Truth lives here + in git commits — never only in an agent's context.

| File | Role | Committed? |
|------|------|-----------|
| `backlog.md` | Single source of truth: ordered work items (`[ ]`/`[x]`/`[!]`). | yes (every cycle) |
| `loop_state.md` | Ledger: counters (`cycle_budget`, `cycles_this_session`, `cycles_total`), current item, status. | yes (every cycle) |
| `HANDOFF.md` | Cold-start resume packet written by `continuity-steward` at a budget. **Authoritative resume signal.** | yes (at handoff) |
| `<cycle>_*` | Per-cycle agent artifacts (architect plan, implementer notes, verification report, docs notes). Preserved for audit. | yes |
| `DONE` / `NEEDS-HUMAN` / `STOP` | Terminal sentinels read by the external runner (one per process). | DONE/NEEDS-HUMAN yes; STOP is a local human action |
| `ralph-run-*.log` | Per-run process logs from the external runner. | **no** (git-ignored) |

## How it runs
- Interactive: invoke `/prompt-loop` (DISCOVER → cycles → handoff at budget) or
  `/prompt-loop resume from _workspace/HANDOFF.md` in a new session.
- Unattended: `bash .claude/skills/prompt-loop/scripts/ralph-prompt.sh` (SAFE: local commits only)
  or `PROMPT_APPLY=1 …` (push → PR → auto-merge on green). `touch _workspace/STOP` to halt.

See `.claude/skills/prompt-loop/SKILL.md` for the full workflow and `_GENERIC` pattern in
`~/Desktop/meta/HARNESS-UPGRADE-KIT.md`.
