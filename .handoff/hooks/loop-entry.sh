#!/usr/bin/env bash
# SessionStart hook — rehydrate prompt_hub's continuity state and auto-invoke the loop.
#
# prompt_hub is a Tier-B FLEET member (ADR-0004 §3): it has NO local ledger.db.
# Its resume packet is compiled centrally by `hf fleet render prompt_hub` (run from
# the meta root) from the git-text cards + the FLEET ledger (meta/.handoff).
#
# Behavior: regenerate + print the member packet (becomes session context), then —
# only when an unblocked backlog card exists — emit a directive telling Claude to run
# the `prompt-loop` skill. No open card → no directive (don't force a loop with
# nothing to do). Best-effort: never hard-fail a hook.
set -uo pipefail
REPO="${CLAUDE_PROJECT_DIR:-.}"
cd "$REPO" 2>/dev/null || exit 0

# Locate hf (PATH, then a sibling kernel build). Never hard-fail.
HF=""
if command -v hf >/dev/null 2>&1; then HF="hf"
elif [ -x ../handoff/target/release/hf ]; then HF="../handoff/target/release/hf"
elif [ -x ../handoff/target/debug/hf ];   then HF="../handoff/target/debug/hf"
fi
if [ -z "$HF" ]; then
  echo "[handoff] hf not on PATH — install it to enable the loop (see meta/handoff/FLEET_GUIDE.md)."
  exit 0
fi

# Find the meta root (parent holding .meta.yaml + the FLEET ledger) and render this
# member's packet from there (the member model — never a local ledger).
META="$(cd .. 2>/dev/null && pwd)"
if [ -f "$META/.meta.yaml" ] && [ -d "$META/.handoff" ]; then
  ( cd "$META" && "$HF" fleet render prompt_hub >/dev/null 2>&1 ) || true
fi

# Print the freshly rendered packet (rehydration → session context).
[ -f .handoff/packets/latest.md ] && cat .handoff/packets/latest.md 2>/dev/null || true

# Detect a safe next card: first status:"backlog" with empty blocked_by, lowest priority.
NEXT="$(python3 - <<'PY' 2>/dev/null || true
import json, glob
best=None; order={"P0":0,"P1":1,"P2":2,"P3":3}
for f in sorted(glob.glob(".handoff/tasks/*.task.json")):
    try: c=json.load(open(f))
    except Exception: continue
    if c.get("status")!="backlog": continue
    if c.get("blocked_by"): continue
    k=order.get(c.get("priority","P3"),3)
    if best is None or k<best[0]: best=(k,c.get("id",""))
print(best[1] if best else "")
PY
)"

if [ -n "$NEXT" ]; then
  cat <<EOF

[prompt-loop] Backlog has a safe next card: ${NEXT}.
ACTION: invoke the \`prompt-loop\` skill to continue the autonomous construction-crew
loop (architect → implement ↔ verify → docs → commit → push/PR/auto-merge), one card
per cycle. The packet above is rendered state and may be stale — the loop re-derives
truth from Git > FLEET ledger > .handoff/tasks/*.task.json first.
Skip ONLY if the user's first request is a one-off question unrelated to the loop.
EOF
fi
exit 0
