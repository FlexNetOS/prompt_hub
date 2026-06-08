---
name: prompt-loop
description: "Autonomous construction crew that continuously upgrades and adds features to prompt_hub — one backlog item per cycle, verified and committed, with fresh-session handoff and optional unattended self-restart. ALWAYS use to: build/add/wire prompt_hub features, run the dev loop, 'work the backlog', 'upgrade prompt_hub', 'continuous feature development'. Follow-up/continuation triggers: 'resume', 'pick up the loop', 'continue in a new session', 're-run', 'next cycle', 'keep going', 'run again'. Defaults to APPLY (push → PR → auto-merge on green DONE-gates, fail-closed); pass 'safe' (or 'dry-run'/'local') for local-commits-only. This is the DEV harness that builds prompt_hub — NOT prompt_hub's product runtime."
---

# Prompt-Loop — Autonomous prompt_hub Construction Crew

The orchestrator. It runs prompt_hub feature development as a **chain of short cycles**: discover real work → build one item with an agent team → verify across boundaries → commit → hand off to a fresh session at a budget → optionally self-restart unattended. Truth lives on disk (`_workspace/backlog.md` + `loop_state.md` + commits), so any restart resumes cold with zero loss.

> **Scope guard:** this harness *builds* prompt_hub. It is not prompt_hub's own agent/Junie product runtime — keep the two separate. The "how to build one feature" discipline is the `feature-build` skill; this skill is "what to build next and how the loop runs".

## Execution Mode: Hybrid
| Stage | Mode | Members |
|-------|------|---------|
| DISCOVER / cycle-start refresh | Sub-agent | `backlog-curator` |
| Per-cycle feature build | **Agent team** | `feature-architect` → `rust-implementer` ↔ `verification-gate` → `docs-scribe` |
| Handoff at budget | Sub-agent | `continuity-steward` (via `session-relay`) |

Only one team is active at a time; the per-cycle team is created and disbanded each cycle so each cycle starts lean.

## Agent Composition
| Member | Agent type | Role | Skill | Output (`_workspace/`) |
|--------|-----------|------|-------|------------------------|
| backlog-curator | general-purpose | Discover/maintain backlog from real state | — | `backlog.md` |
| feature-architect | Plan | Blast radius + Rust-native design plan | feature-build | `<cycle>_architect_plan.md` |
| rust-implementer | rust-implementer | Core-first implementation + tests | feature-build | `<cycle>_implementer_notes.md` |
| verification-gate | general-purpose | Cross-boundary QA + both-config gates | feature-build | `<cycle>_verification_report.md` |
| docs-scribe | docs-scribe | Docs/ADR/changelog sync | feature-build | `<cycle>_docs_notes.md` |
| continuity-steward | general-purpose | Cold-start handoff | — | `HANDOFF.md` |

> Always invoke every agent with `model: "opus"`.

## Apply policy (CODE loop — "apply" = git ops, no system mutation)

**Default = APPLY.** Invoking `/prompt-loop` (or resuming it) defaults to the full apply path each
cycle: build + commit → **push** the feature branch → **open a PR** (evidence in body) →
**auto-merge ONLY when the full DONE-criteria gate suite is green** for that feature. Pass an
explicit `safe` (synonyms: `dry-run`, `local`) to stay local. Fail-closed: if branch protection /
required CI blocks the self-merge, or the permission sandbox denies a `git`/`gh` command, write
`_workspace/NEEDS-HUMAN` (reason inside) — never `--force`, never weaken protection or a guard.

| Mode | Trigger | What the loop may do |
|------|---------|----------------------|
| **Apply** (default) | `/prompt-loop` with no override · external runner with `PROMPT_APPLY=1` | Build + commit per cycle → push → PR → auto-merge on green DONE-gates (fail-closed to `NEEDS-HUMAN`). |
| **Safe** (explicit override) | `/prompt-loop safe` (or `dry-run`/`local`) · external runner with `PROMPT_APPLY` unset | Build + commit to a local feature branch only. **Never** push, PR, or merge. |

Auto-merge is gated on *proven* green (build + test + lint + fmt-clean), uses a safe squash merge
(`gh pr merge --squash`), and stops at the first failure. In an interactive session the **permission
sandbox still backstops** every push/merge — they prompt unless you allowlist the commands in
`.claude/settings.json`. The headless **runner keeps apply as a deliberate `PROMPT_APPLY=1` opt-in**
(per the kit's "safe by default" principle) so an unattended self-restart never escalates by
accident; the human-invoked slash command, where you are present and authorized, defaults to apply.

## Workflow

### Phase 0: Context Check (initial / resume / partial re-run)
1. Look for `_workspace/HANDOFF.md` and `_workspace/backlog.md`.
   - **`HANDOFF.md` present, or trigger says "resume"** → invoke `session-relay` **RESUME** (read handoff → run verify-on-resume baseline → reset `cycles_this_session=0` → continue at the current backlog item). Skip DISCOVER.
   - **`backlog.md` present, user requests a specific item / partial re-run** → skip DISCOVER; jump to Phase 2 for that item.
   - **Neither present** → Phase 1 DISCOVER.
2. Read `_workspace/loop_state.md` for counters (`cycle_budget`, `cycles_this_session`, `cycles_total`).
3. **Resolve apply mode** (see Apply policy): default **Apply**; if the invocation includes `safe`/`dry-run`/`local`, use **Safe**; an external runner's explicit `PROMPT_APPLY` value wins for that entry point. Record the resolved mode in `loop_state.md`.

### Phase 1: DISCOVER (initial only)
1. Spawn `backlog-curator` (sub-agent, opus) → it reads real state (TODO.md, docs/audits, staged features, `gh` issues/PRs, gate gaps) and writes `_workspace/backlog.md` (ordered, with provenance), seeding `loop_state.md`.
2. Do **not** build during DISCOVER. Commit the seeded state (`chore(loop): discover backlog`).

### Phase 2: One Cycle (the iteration)
Run for each cycle until a stop condition:

**a. Stop-checks (before building):**
- No `- [ ]` remain in `backlog.md` → go to **DONE** (Phase 3).
- `cycles_this_session >= cycle_budget` → go to **HAND OFF** (Phase 4).
- A prior `_workspace/STOP` or `NEEDS-HUMAN` exists → halt.

**b. Pick the top unblocked item** (respect dependency order). Set `loop_state.last_item`.

**c. Build it (agent team):**
1. `TeamCreate(team_name:"prompt-build", members:[feature-architect, rust-implementer, verification-gate, docs-scribe])`, all `model:"opus"`.
2. `TaskCreate` the cycle's tasks with dependencies:
   - plan (architect) → implement (implementer, depends plan) → verify (verification-gate, depends implement, **incremental** per module) → document (docs-scribe, depends verify pass).
3. Members self-coordinate via `SendMessage` (architect↔implementer on design gaps; implementer↔verifier produce/review loop; implementer→docs-scribe on user-facing changes). They follow the `feature-build` discipline.
4. Leader (this skill) monitors via `TaskGet`; intervenes/reassigns on idle or block.
5. Disband the team (`TeamDelete`) once `verification-gate` reports `pass` and docs are synced. `_workspace/` artifacts persist.

**d. VERIFY across the boundary (leader re-confirms, fresh shell):** re-run the both-config gates (`cargo check --workspace`; `just test`; `just lint`) yourself — don't trust an in-context "green". Confirm the verification report is `pass` with evidence (not existence-only).

**e. Write state back + commit (one cohesive commit):**
- Mark the item `- [x]` (with commit/PR evidence) or `- [!] blocked: <reason>` in `backlog.md`; bump `cycles_this_session` and `cycles_total`; update `last_update`.
- Commit code + docs + `_workspace/{backlog,loop_state}.md` together, Conventional-Commit subject, ending with the `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>` trailer.
- Apply per the resolved mode (**Apply policy**): default **Apply** → push → PR → auto-merge on green DONE-gates (fail-closed to `NEEDS-HUMAN` if blocked/sandbox-denied); **Safe** override → local commit only.

**f. Self-pace:** in an interactive session, `ScheduleWakeup` to re-enter the next cycle (long delay if waiting on a slow external step like CI). Under the external runner, do **not** wake — finish the budget then write one sentinel.

### Phase 3: DONE (no `- [ ]` left)
Run the **DONE-criteria suite** (`cargo build --workspace --all-features` · `just test` · `just lint` · `just fmt && git diff --quiet`). All green + backlog empty → write `_workspace/DONE` with the evidence (commands + results + landed commits/PRs). This is the terminal sentinel; stop (no wakeup). If any gate is red, the backlog isn't really empty — add a fix item and continue.

### Phase 4: HAND OFF (budget reached) — Handoff Ledger V2
Invoke `session-relay` **HAND OFF** with handoff packet compilation:

1. **Emit session event.** Record `session_stopped` event via mesh heartbeat (`relay:handoff`).
2. **Compile Handoff Packet V2** from current Git state, gate results, and backlog → per `.claude/skills/prompt-loop/handoff/schemas/packet.schema.json`.
3. **Spawn continuity-steward** → writes `_workspace/HANDOFF.md` containing the packet as a markdown JSON block + human-readable summary.
4. **Commit.** `git add _workspace/HANDOFF.md _workspace/backlog.md _workspace/loop_state.md && git commit` (`chore(loop): handoff cycle N`).
5. **Heartbeat** (best-effort mesh relay). Skip silently if unavailable.
6. **Stop.** Under the external runner: write exactly one sentinel (`HANDOFF.md` = more work remains → respawn; `DONE` = finished; `NEEDS-HUMAN` = human wall).

## Data Flow
```
backlog-curator → backlog.md
        │  (top item)
        ▼
[TeamCreate prompt-build]
 feature-architect ──plan──▶ rust-implementer ──module──▶ verification-gate
        ▲   design gap            ▲  fix req (file:line)        │ pass
        └────────────────────────┘                             ▼
                                              rust-implementer → docs-scribe
        │ (all _workspace/<cycle>_* artifacts persist)
        ▼
[Leader] re-verify (fresh shell) → mark backlog → COMMIT → apply-policy → next cycle
        │ at budget                                   │ backlog empty + gates green
        ▼                                             ▼
 session-relay HAND OFF (continuity-steward)     _workspace/DONE
```

## Error Handling
| Situation | Strategy |
|-----------|----------|
| A team member stalls/fails | Leader detects via TaskGet/idle → SendMessage to check → restart or reassign its task; note partial result, don't discard |
| Verify fails | Send specific fix request to rust-implementer (file:line + how); re-verify. >2–3 unconverging rounds → mark `- [!] blocked` with the reason, move on |
| Guard would need weakening to pass | **Stop.** Never weaken `-D warnings`/a test/`#![forbid(unsafe_code)]`. Fix the cause or block the item honestly |
| Human wall (interactive auth, irreversible op, branch protection on self-merge) | Write `_workspace/NEEDS-HUMAN` with the reason; halt — never force |
| Conflicting data between members | Keep both, cite sources; let the architect adjudicate the design |
| Single cycle exceeds budget mid-build | Finish/commit the current item if safe, else record honest partial state, then HAND OFF |

Bounds: the external runner enforces `MAX_ITERS` and an always-checked `_workspace/STOP` kill switch. Retry a transient step once; on a second failure proceed without it and record the omission.

## Test Scenarios
### Happy path
1. Fresh worktree, no `_workspace/HANDOFF.md` → Phase 1 DISCOVER seeds `backlog.md` from real state, commits.
2. Phase 2: top item → team plans/implements/verifies/documents → leader re-verifies green → marks `- [x]` → commits (interactive: local; APPLY: push→PR→auto-merge on green).
3. Repeat until `cycles_this_session == cycle_budget` → Phase 4 writes+commits `HANDOFF.md` and stops.
4. New session: `/prompt-loop resume from _workspace/HANDOFF.md` → RESUME runs verify baseline, resets counter, continues at the next item.
5. Eventually no `- [ ]` left + DONE-suite green → `_workspace/DONE` written; loop terminates.

### Error path
1. During a cycle, `verification-gate` reports a core-API↔server boundary mismatch (file:line both sides).
2. Leader relays the fix request to `rust-implementer`; it patches; verifier re-checks → still red after 3 rounds.
3. Leader marks the item `- [!] blocked: <reason>`, commits the honest state, and either picks the next unblocked item or (if a human wall) writes `NEEDS-HUMAN`.
4. No false green is ever written; the blocked reason survives in `backlog.md` for the next session.

## External self-restart
The `/new` effect (a fresh process = clean context) is provided by `scripts/ralph-prompt.sh` — a bounded `while` loop that spawns one fresh `claude -p "/prompt-loop resume …"` per iteration, reads the one sentinel it wrote, and respawns until `DONE`/`NEEDS-HUMAN`/`STOP`. Safe by default; `PROMPT_APPLY=1` opts into push/PR/auto-merge; `touch _workspace/STOP` halts it.
