# Loop state — prompt-loop
session_started: (set at first run — you supply UTC; scripts can't read the clock)
loop: prompt-loop
branch: (set per cycle — feature branch in the active worktree)
worktree: (abs path of the worktree the loop runs in)
cycle_budget: 3            # completed cycles per session before handoff (override via PROMPT_BUDGET)
cycles_this_session: 0     # reset to 0 on RESUME
cycles_total: 0            # carried across sessions
last_item: (none — discovery not yet run)
status: SEED — backlog not yet discovered (run /prompt-loop to DISCOVER)
last_update: (set on first write)
