#!/usr/bin/env bash
# ralph-prompt.sh — external Ralph loop for prompt_hub's construction crew.
# Self-restarts /prompt-loop with a FRESH context each iteration (each `claude -p`
# process is a clean session = the "/new" effect) until a terminal sentinel.
#
# Truth lives on disk: _workspace/backlog.md + loop_state.md + commits + HANDOFF.md.
# Each fresh agent runs up to BUDGET cycles, writes EXACTLY ONE sentinel, and exits.
#
# ── Permission model (read this) ──────────────────────────────────────────────
# This runner deliberately does NOT disable the Claude Code permission sandbox.
# It does not pass --dangerously-skip-permissions. Spawned agents run under your
# normal permission rules, so destructive/outbound actions still go through the
# approval mechanism you configured. For genuinely unattended operation, the
# OPERATOR must pre-authorize the specific commands the loop needs by adding an
# allowlist to .claude/settings.json (e.g. cargo/just/git/gh subcommands) — the
# sanctioned path — rather than turning the sandbox off wholesale. Granting that
# allowlist is an explicit, reviewable decision left to you.
#
#   SAFE (default):  bash .claude/skills/prompt-loop/scripts/ralph-prompt.sh
#                    -> build + commit locally only; never push/PR/merge.
#   APPLY (opt-in):  PROMPT_APPLY=1 bash .claude/skills/prompt-loop/scripts/ralph-prompt.sh
#                    -> push branch -> open PR -> auto-merge ONLY when the full
#                       DONE-criteria gates are green; fail-closed to NEEDS-HUMAN otherwise.
#   Kill switch:     touch _workspace/STOP   (halts before the next spawn, any time)
set -euo pipefail

WORKTREE="${PROMPT_WORKTREE:-$(pwd)}"
BUDGET="${PROMPT_BUDGET:-3}"            # completed cycles per fresh session before handoff
MAX_ITERS="${PROMPT_MAX_ITERS:-50}"    # hard backstop on spawns
SLEEP_BETWEEN="${PROMPT_SLEEP:-5}"
MODEL="${PROMPT_MODEL:-opus}"
WS="$WORKTREE/_workspace"; mkdir -p "$WS"

log(){ printf '[ralph-prompt %s] %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; }
command -v claude >/dev/null || { log "FATAL: claude not on PATH"; exit 1; }

if [ "${PROMPT_APPLY:-0}" = "1" ]; then
  log "APPLY MODE — agents are permitted (per their prompt) to push, open PRs, and auto-merge on"
  log "green gates. They still run under your permission rules; allowlist the needed commands in"
  log ".claude/settings.json for an unattended run. The sandbox is NOT disabled by this script."
else
  log "SAFE mode (default): local commits only. No push/PR/merge. Set PROMPT_APPLY=1 to enable apply."
fi

read -r -d '' PROMPT <<EOF || true
/prompt-loop resume (external Ralph runner, fresh context). Worktree: $WORKTREE. Cycle budget: $BUDGET.
Apply mode: ${PROMPT_APPLY:-0} (0 = SAFE local-commits-only; 1 = push -> PR -> auto-merge ONLY on green DONE-gates).
1. If _workspace/HANDOFF.md exists, follow session-relay RESUME from it (authoritative signal):
   read it, run the verify-on-resume baseline, reset cycles_this_session=0, continue at the current item.
   Else if _workspace/backlog.md is missing, run DISCOVER (backlog-curator) to seed it.
2. Run up to $BUDGET cycles of /prompt-loop Phase 2: one backlog item each via the agent team
   (feature-architect -> rust-implementer <-> verification-gate -> docs-scribe, all model opus),
   following the feature-build discipline. VERIFY across boundaries in a FRESH shell, both default
   and --all-features. Commit per cycle (Conventional Commit + Co-Authored-By trailer). Fail-closed;
   never weaken a guard. Honor the apply mode exactly. If an action is blocked by the permission
   sandbox, treat it as a human wall: write NEEDS-HUMAN with the blocked command rather than forcing it.
3. Then write EXACTLY ONE sentinel under _workspace/ and stop (do NOT ScheduleWakeup):
   - _workspace/DONE        (with evidence)  — backlog empty AND full DONE-gates green
   - _workspace/NEEDS-HUMAN (with reason)    — a human wall (interactive auth / irreversible op /
                                               branch-protection blocking self-merge / sandbox denial)
   - else _workspace/HANDOFF.md              — more work remains (spawn continuity-steward)
EOF

cd "$WORKTREE"; i=0
while :; do
  i=$((i+1)); [ "$i" -gt "$MAX_ITERS" ] && { log "MAX_ITERS hit — halting."; exit 3; }
  [ -f "$WS/STOP" ]        && { log "STOP — halting."; exit 2; }
  [ -f "$WS/DONE" ]        && { log "DONE."; exit 0; }
  [ -f "$WS/NEEDS-HUMAN" ] && { log "NEEDS-HUMAN: $(cat "$WS/NEEDS-HUMAN")"; exit 2; }
  log "iter $i/$MAX_ITERS — spawning fresh agent (budget=$BUDGET, model=$MODEL)"
  claude -p "$PROMPT" --model "$MODEL" --add-dir "$WORKTREE" \
    >>"$WS/ralph-run-$i.log" 2>&1 || log "iter $i exited nonzero (continuing from durable state)"
  [ -f "$WS/DONE" ]        && { log "DONE."; exit 0; }
  [ -f "$WS/NEEDS-HUMAN" ] && { log "NEEDS-HUMAN: $(cat "$WS/NEEDS-HUMAN")"; exit 2; }
  [ -f "$WS/STOP" ]        && { log "STOP — halting."; exit 2; }
  sleep "$SLEEP_BETWEEN"
done
