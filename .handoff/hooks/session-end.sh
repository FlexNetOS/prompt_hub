#!/usr/bin/env bash
# SessionEnd hook — continuity safety net for prompt_hub (FLEET member).
#
# If a session ends without the loop reaching its own checkpoint/handoff step,
# re-render the member packet so the next session resumes from truth, not from a
# half-finished turn. MEMBER MODEL (ADR-0004 §3): no local ledger.db — the packet is
# compiled by `hf fleet render prompt_hub` from the meta root.
#
# Idempotent and best-effort: never block session teardown.
set -uo pipefail
REPO="${CLAUDE_PROJECT_DIR:-.}"
cd "$REPO" 2>/dev/null || exit 0

HF=""
if command -v hf >/dev/null 2>&1; then HF="hf"
elif [ -x ../handoff/target/release/hf ]; then HF="../handoff/target/release/hf"
elif [ -x ../handoff/target/debug/hf ];   then HF="../handoff/target/debug/hf"
fi
[ -z "$HF" ] && exit 0

# Re-render the member packet/active from cards + FLEET ledger truth.
META="$(cd .. 2>/dev/null && pwd)"
if [ -f "$META/.meta.yaml" ] && [ -d "$META/.handoff" ]; then
  ( cd "$META" && "$HF" fleet render prompt_hub >/dev/null 2>&1 ) || true
fi
exit 0
