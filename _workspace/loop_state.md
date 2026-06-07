# Loop state — prompt-loop
session_started: 2026-06-07T00:00:00Z   # all epic work complete — next session needs DISCOVER
loop: prompt-loop
branch: main (primary checkout)
worktree: none (merged to origin/main)
cycle_budget: 3            # completed cycles per session before handoff (override via PROMPT_BUDGET)
cycles_this_session: 2     # cycles 1-2 complete (swarm + quality_gate)
cycles_total: 16           # sessions 1-6 + s7c1+c2
apply_mode: APPLY          # push -> PR -> DIRECT squash merge on green DONE-gates
last_item: Wire quality_gate::QualityGate into PromptHub (✅ merged as #50)
status: C2 complete. Next: wire lineage::LineageTracker. 1 cycle remains this session.
last_update: 2026-06-07T18:10:00Z
# Gates at s7c2 completion (see _workspace/c2_*):
#   check: 3 crates ✅ | clippy (--all-targets -D warnings): clean ✅ | fmt: clean ✅ | tests: 689/685 ✅
