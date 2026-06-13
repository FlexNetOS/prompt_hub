---
name: prompt-loop
description: "Autonomous construction crew that continuously upgrades and adds features to prompt_hub — one backlog item per cycle, verified and committed, with fresh-session handoff and optional unattended self-restart. ALWAYS use to: build/add/wire prompt_hub features, run the dev loop, 'work the backlog', 'upgrade prompt_hub', 'continuous feature development'. Follow-up/continuation triggers: 'resume', 'pick up the loop', 'continue in a new session', 're-run', 'next cycle', 'keep going', 'run again'. Defaults to APPLY (push → PR → auto-merge on green DONE-gates, fail-closed); pass 'safe' (or 'dry-run'/'local') for local-commits-only. This is the DEV harness that builds prompt_hub — NOT prompt_hub's product runtime."
---

# Prompt-Loop — Autonomous prompt_hub Construction Crew

The orchestrator. It runs prompt_hub feature development as a **chain of short cycles**: discover real work → build one item with an agent team → verify across boundaries → commit → hand off to a fresh session at a budget → optionally self-restart unattended. Truth lives on disk in the **`.handoff/` Continuity Ledger Kernel layer** (task cards + derived packet + witnessed FLEET-ledger events + commits), so any restart resumes cold with zero loss.

> **Scope guard:** this harness *builds* prompt_hub. It is not prompt_hub's own agent/Junie product runtime — keep the two separate. The "how to build one feature" discipline is the `feature-build` skill; this skill is "what to build next and how the loop runs".

## State backend — `.handoff/` (CANONICAL; supersedes `_workspace/`)

> **Adopted 2026-06-13 (owner directive).** The durable state moved from the deprecated
> `_workspace/{backlog,loop_state,HANDOFF}.md` to the canonical handoff kernel (`hf` + `.handoff/`,
> per `meta/handoff/FLEET_GUIDE.md`). **Wherever a step below names a `_workspace/...` path, use the
> `.handoff/` equivalent in this table instead** — the inline names are retained only for narrative.

| Concept | OLD (`_workspace/`) | NEW (canonical) |
|---------|---------------------|-----------------|
| Backlog item | a `- [ ]` line in `backlog.md` | a card `.handoff/tasks/PHTASK-NNNN.task.json` (`handoff.task.v1`); `status` ∈ backlog/active/claimed/blocked/checkpointed/review/done |
| Pick next | top unchecked line | top `status:"backlog"` card by priority (P0<P1<P2<P3), `blocked_by` empty |
| Mark done | `- [x]` | set the card's `status:"done"`, then `cd <meta-root> && hf fleet render prompt_hub` |
| Counters | `loop_state.md` | `.handoff/active.md` (pointer) + witnessed events in the FLEET ledger (`meta/.handoff`) |
| Resume signal | `HANDOFF.md` present | `.handoff/packets/latest.md` (derived — `hf fleet render prompt_hub`; **never hand-edit**) |
| Sentinels | `_workspace/{STOP,DONE,NEEDS-HUMAN}` | `.handoff/{STOP,DONE,NEEDS-HUMAN}` (control files; same semantics) |
| Cycle scratch | `_workspace/<cycle>_*.md` | `.handoff/work/<cycle>_*.md` (ephemeral; not committed state) |

**Rules:** `.handoff/` is git-text-only — **never** create a `ledger.db` here (events live in the
FLEET ledger; `hf fleet status` flags a stray per-repo ledger as a P7 violation). Do **not** run
`hf init`/`hf seed` in this repo. State precedence: `Git > FLEET ledger > tasks/*.task.json >
active.md > packets/latest.md`. New backlog cards are authored as git-text (the migration generator
is `.handoff/history/generate_cards.py`; blake3 `intent_lock`s must match `hf`).

## Execution Mode: Hybrid
| Stage | Mode | Members |
|-------|------|---------|
| DISCOVER / cycle-start refresh | Sub-agent | `backlog-curator` |
| Per-cycle feature build | **Agent team** | `feature-architect` → `rust-implementer` ↔ `verification-gate` → `docs-scribe` |
| Handoff at budget | Sub-agent | `continuity-steward` (via `session-relay`) |

Only one team is active at a time; the per-cycle team is created and disbanded each cycle so each cycle starts lean.

## Agent Composition
| Member | Agent type | Role | Skill | Output |
|--------|-----------|------|-------|--------|
| backlog-curator | general-purpose | Discover/maintain backlog from real state | — | `.handoff/tasks/PHTASK-*.task.json` |
| feature-architect | Plan | Blast radius + Rust-native design plan | feature-build | `.handoff/work/<cycle>_architect_plan.md` |
| rust-implementer | rust-implementer | Core-first implementation + tests | feature-build | `.handoff/work/<cycle>_implementer_notes.md` |
| verification-gate | general-purpose | Cross-boundary QA + both-config gates | feature-build | `.handoff/work/<cycle>_verification_report.md` |
| docs-scribe | docs-scribe | Docs/ADR/changelog sync | feature-build | `.handoff/work/<cycle>_docs_notes.md` |
| continuity-steward | general-purpose | Cold-start handoff | — | `.handoff/packets/latest.md` (via `hf fleet render`) |

> Always invoke every agent with `model: "opus"`.

## Apply policy (CODE loop — "apply" = git ops, no system mutation)

**Default = APPLY.** Invoking `/prompt-loop` (or resuming it) defaults to the full apply path each
cycle: build + commit → **push** the feature branch → **open a PR** (evidence in body) →
**auto-merge ONLY when the full DONE-criteria gate suite is green** for that feature. Pass an
explicit `safe` (synonyms: `dry-run`, `local`) to stay local. Fail-closed: if branch protection /
required CI blocks the self-merge, or the permission sandbox denies a `git`/`gh` command, write
`.handoff/NEEDS-HUMAN` (reason inside) — never `--force`, never weaken protection or a guard.

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
1. Read the canonical state: `.handoff/context/capsule.json`, `.handoff/packets/latest.md` (run `cd <meta-root> && hf fleet render prompt_hub` first to refresh it), and the cards in `.handoff/tasks/`. `hf resume` (from the meta root) is the one-shot equivalent.
   - **Cards exist (any `status:"backlog"`), or trigger says "resume"** → invoke `session-relay` **RESUME** (read the packet → run verify-on-resume baseline → reset the per-session counter → continue at the top unblocked card). Skip DISCOVER.
   - **User requests a specific card / partial re-run** → skip DISCOVER; jump to Phase 2 for that `PHTASK-NNNN`.
   - **No `.handoff/tasks/` cards at all** → Phase 1 DISCOVER.
2. Read counters from `.handoff/active.md` + the FLEET ledger (`hf status`/`hf resume`): `cycle_budget` (skill default 5), the per-session cycle count, totals.
3. **Resolve apply mode** (see Apply policy): default **Apply**; if the invocation includes `safe`/`dry-run`/`local`, use **Safe**; an external runner's explicit `PROMPT_APPLY` value wins for that entry point. Record the resolved mode in `.handoff/active.md`.

### Phase 1: DISCOVER (initial only)
1. Spawn `backlog-curator` (sub-agent, opus) → it reads real state (TODO.md, docs/audits, staged features, `gh` issues/PRs, gate gaps) and authors `handoff.task.v1` cards in `.handoff/tasks/` (each with an `hf`-identical blake3 `intent_lock`; see `.handoff/history/generate_cards.py` for the pattern), then `hf fleet render prompt_hub` to seed `.handoff/packets/latest.md` + `.handoff/active.md`.
2. Do **not** build during DISCOVER. Commit the seeded state (`chore(loop): discover backlog → .handoff cards`).

### Phase 2: One Cycle (the iteration)
Run for each cycle until a stop condition:

**a. Stop-checks (before building):**
- No `status:"backlog"` cards remain in `.handoff/tasks/` → go to **DONE** (Phase 3).
- per-session cycles `>= cycle_budget` → go to **HAND OFF** (Phase 4).
- A prior `.handoff/STOP` or `.handoff/NEEDS-HUMAN` exists → halt.

**b. Pick the top unblocked card** — lowest priority number (P0<P1<P2<P3) among `status:"backlog"` with empty `blocked_by`. Record it in `.handoff/active.md` (and `hf claim <PHTASK-NNNN>` from the meta root if witnessing to the FLEET ledger).

**c. Build it (agent team):**
1. `TeamCreate(team_name:"prompt-build", members:[feature-architect, rust-implementer, verification-gate, docs-scribe])`, all `model:"opus"`.
2. `TaskCreate` the cycle's tasks with dependencies:
   - plan (architect) → implement (implementer, depends plan) → verify (verification-gate, depends implement, **incremental** per module) → document (docs-scribe, depends verify pass).
3. Members self-coordinate via `SendMessage` (architect↔implementer on design gaps; implementer↔verifier produce/review loop; implementer→docs-scribe on user-facing changes). They follow the `feature-build` discipline.
4. Leader (this skill) monitors via `TaskGet`; intervenes/reassigns on idle or block.
5. Disband the team (`TeamDelete`) once `verification-gate` reports `pass` and docs are synced. `.handoff/work/` artifacts persist.

**d. VERIFY across the boundary (leader re-confirms, fresh shell):** re-run the both-config gates (`cargo check --workspace`; `just test`; `just lint`) yourself — don't trust an in-context "green". Confirm the verification report is `pass` with evidence (not existence-only).

**e. Write state back + commit (one cohesive commit):**
- Set the card's `status:"done"` (with commit/PR evidence in the card) or `status:"blocked"` + a `block_reason` in `.handoff/tasks/PHTASK-NNNN.task.json`; update `.handoff/active.md`; regenerate the packet: `cd <meta-root> && hf fleet render prompt_hub` (and `hf checkpoint <PHTASK-NNNN> "<note>"` to witness in the FLEET ledger).
- Commit code + docs + `.handoff/{tasks,active.md,packets/latest.md}` together, Conventional-Commit subject, ending with the `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>` trailer.
- Apply per the resolved mode (**Apply policy**): default **Apply** → push → PR → auto-merge on green DONE-gates (fail-closed to `.handoff/NEEDS-HUMAN` if blocked/sandbox-denied); **Safe** override → local commit only.

**f. Self-pace:** in an interactive session, `ScheduleWakeup` to re-enter the next cycle (long delay if waiting on a slow external step like CI). Under the external runner, do **not** wake — finish the budget then write one sentinel.

### Phase 3: DONE (no `status:"backlog"` cards left)
Run the **DONE-criteria suite** (`cargo build --workspace --all-features` · `just test` · `just lint` · `just fmt && git diff --quiet`). All green + no open cards → write `.handoff/DONE` with the evidence (commands + results + landed commits/PRs). This is the terminal sentinel; stop (no wakeup). If any gate is red, the backlog isn't really empty — author a fix card (`.handoff/tasks/PHTASK-NNNN.task.json`, status backlog) and continue.

### Phase 4: HAND OFF (budget reached) — Handoff Ledger V2
Invoke `session-relay` **HAND OFF** with handoff packet compilation:

1. **Emit session event.** Record `session_stopped` event via mesh heartbeat (`relay:handoff`).
2. **Compile the packet** (`handoff.packet.v2`) canonically: `cd <meta-root> && hf fleet render prompt_hub` regenerates `.handoff/packets/latest.md` from the cards + FLEET ledger. (Schema also at `.claude/skills/prompt-loop/handoff/schemas/packet.schema.json`.)
3. **Update `.handoff/active.md`** with the next card + done count; the packet IS the next-session prompt. (`continuity-steward` may add a human-readable summary, but the derived packet is never hand-edited.)
4. **Commit.** `git add .handoff/ && git commit` (`chore(loop): handoff cycle N — render packet + active`).
5. **Heartbeat** (best-effort mesh relay). Skip silently if unavailable.
6. **Stop.** Under the external runner: write exactly one sentinel under `.handoff/` (`HANDOFF` marker / non-empty open cards = more work → respawn; `DONE` = finished; `NEEDS-HUMAN` = human wall).

## Data Flow
```
backlog-curator → .handoff/tasks/PHTASK-*.task.json (cards)
        │  (top unblocked card by priority)
        ▼
[TeamCreate prompt-build]
 feature-architect ──plan──▶ rust-implementer ──module──▶ verification-gate
        ▲   design gap            ▲  fix req (file:line)        │ pass
        └────────────────────────┘                             ▼
                                              rust-implementer → docs-scribe
        │ (cycle scratch in .handoff/work/<cycle>_*)
        ▼
[Leader] re-verify (fresh shell) → card status:done → hf fleet render → COMMIT .handoff/ → apply-policy → next cycle
        │ at budget                                   │ no open cards + gates green
        ▼                                             ▼
 session-relay HAND OFF (hf fleet render packet)  .handoff/DONE
```

## Error Handling
| Situation | Strategy |
|-----------|----------|
| A team member stalls/fails | Leader detects via TaskGet/idle → SendMessage to check → restart or reassign its task; note partial result, don't discard |
| Verify fails | Send specific fix request to rust-implementer (file:line + how); re-verify. >2–3 unconverging rounds → mark `- [!] blocked` with the reason, move on |
| Guard would need weakening to pass | **Stop.** Never weaken `-D warnings`/a test/`#![forbid(unsafe_code)]`. Fix the cause or block the item honestly |
| Human wall (interactive auth, irreversible op, branch protection on self-merge) | Write `.handoff/NEEDS-HUMAN` with the reason; halt — never force |
| Conflicting data between members | Keep both, cite sources; let the architect adjudicate the design |
| Single cycle exceeds budget mid-build | Finish/commit the current item if safe, else record honest partial state, then HAND OFF |

Bounds: the external runner enforces `MAX_ITERS` and an always-checked `.handoff/STOP` kill switch. Retry a transient step once; on a second failure proceed without it and record the omission.

## Test Scenarios
### Happy path
1. Fresh worktree, no `.handoff/tasks/` cards → Phase 1 DISCOVER seeds cards from real state, commits.
2. Phase 2: top card → team plans/implements/verifies/documents → leader re-verifies green → sets card `status:done` → commits (interactive: local; APPLY: push→PR→auto-merge on green).
3. Repeat until the per-session cycle count == `cycle_budget` → Phase 4 renders+commits `.handoff/packets/latest.md` and stops.
4. New session: `/prompt-loop resume` → RESUME reads `.handoff/packets/latest.md` (`hf resume`), runs verify baseline, resets counter, continues at the next card.
5. Eventually no open cards + DONE-suite green → `.handoff/DONE` written; loop terminates.

### Error path
1. During a cycle, `verification-gate` reports a core-API↔server boundary mismatch (file:line both sides).
2. Leader relays the fix request to `rust-implementer`; it patches; verifier re-checks → still red after 3 rounds.
3. Leader sets the card `status:blocked` (+ `block_reason`), commits the honest state, and either picks the next unblocked card or (if a human wall) writes `.handoff/NEEDS-HUMAN`.
4. No false green is ever written; the `block_reason` survives in the card for the next session.

## External self-restart
The `/new` effect (a fresh process = clean context) is provided by `scripts/ralph-prompt.sh` — a bounded `while` loop that spawns one fresh `claude -p "/prompt-loop resume …"` per iteration, reads the one sentinel it wrote, and respawns until `.handoff/DONE`/`NEEDS-HUMAN`/`STOP`. Safe by default; `PROMPT_APPLY=1` opts into push/PR/auto-merge; `touch .handoff/STOP` halts it.
