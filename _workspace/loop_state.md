# Loop state — prompt-loop
session_started: 2026-06-05T22:27:26Z
loop: prompt-loop
branch: feat/cli-metrics (cycle-2 feature branch; base origin/main)
worktree: /home/drdave/Desktop/meta/.worktrees/harness-crew
cycle_budget: 3            # completed cycles per session before handoff (override via PROMPT_BUDGET)
cycles_this_session: 2     # reset to 0 on RESUME
cycles_total: 2            # carried across sessions
apply_mode: APPLY          # push -> PR -> auto-merge on green DONE-gates (fail-closed)
last_item: CLI — add `prompthub metrics` subcommand (Prometheus exposition, cfg otel)
status: cycle 2 complete — metrics subcommand verified green (build default+otel+all-features, clippy, fmt, tests); committing + PR
last_update: 2026-06-05T22:44:30Z
# Cycle ledger:
#   c1 (PR #30, merged cddff47): P0 fix audit.rs sha2 0.11 LowerHex -> green build
#   c2 (PR #__, this cycle):     prompthub metrics CLI subcommand
