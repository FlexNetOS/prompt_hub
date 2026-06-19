# prompt_hub loop — LESSONS (append-only)

## 2026-06-13
- **L1 Stale-backlog drift.** The migrated 2026-06-07 audit cards claim modules are
  "dead/empty/no-impl"; several (multimodal_input.rs, templates.rs) were already fully
  implemented. Real gap = WIRING into the hub. Always verify code-vs-prose (code wins).
- **L2 No downgrades.** Owner standing rule: a stub/no-op/zero-caller module is an
  incomplete FEATURE to complete — never remove/gate-out/stub. Cards are completion-only.
- **L3 Worktree-per-cycle.** Concurrent sessions switch the shared checkout's branch under
  you (confirmed mid-session). Always `git worktree add` off `origin/main`; never edit the
  shared `meta/prompt_hub` checkout.
- **L4 hf member-model packet render.** `hf fleet render prompt_hub` resolves the member to
  `meta/prompt_hub` (main checkout), never a worktree. A worktree cycle CANNOT render its own
  packet. Commit cards + active.md + code from the worktree; the packet is a derived view that
  regenerates on the next `/prompt-loop resume` Phase 0 from the merged cards (cards = truth).
- **L5 default-features blind spot.** All CI jobs used `--all-features`, masking a
  default-features-only break (PHTASK-0028). Added a `default-build` CI job as the guard;
  a "pre-existing" failure found via `git stash` check still warrants a regression job.
- **L6 Cycle serialization.** Each cycle touches the shared `.handoff` packet/active, so cycle
  PRs serialize. Branch each cycle off the LATEST main; let the packet regenerate.
