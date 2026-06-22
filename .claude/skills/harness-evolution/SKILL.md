---
name: harness-evolution
description: >-
  How to turn a prompt-loop run into harness upgrades — evaluate the run from the .handoff/ kernel
  artifacts, mine generalizable lessons, route each to the right target (skill / agent / orchestrator
  / description / script), and apply (low-risk, in-scope) or propose (structural) the change,
  fail-closed. ALWAYS use at the end of any prompt-loop run (DONE or HAND OFF) and on "evaluate the
  run", "retro", "what did we learn", "improve the harness", "upgrade the harness", "apply lessons",
  "why did the loop struggle". The harness's self-improvement method (automates the harness skill's
  Phase 7). Run by the evolution-steward agent.
---

# Harness Evolution (prompt-loop)

A harness gets better only if each run teaches the next one. This skill is the method the
`evolution-steward` runs at every run boundary: **evaluate → mine lessons → route → apply/propose →
record**. Done well it compounds; done carelessly it overfits or weakens the harness — so the rules
below are about *safe* improvement.

> **Kernel-native artifacts.** This crew stores durable state in the **`.handoff/` Continuity Ledger
> Kernel** layout, not the file-based `.handoff/loop/` layout the generic version assumes. Reconstruct
> the run from: `.handoff/tasks/PHTASK-*.task.json` cards (outcome/`block_reason`/evidence),
> `.handoff/work/<cycle>_*.md` (per-cycle crew scratch), `.handoff/active.md` (counters/next), the
> **FLEET ledger** (`cd <meta-root> && hf resume` / `hf status` / `hf drift`), `.handoff/packets/latest.md`
> (derived), and the **git/PR commit log**. There is no `loop_state.md` / `findings/` / `HANDOFF.md`.

## 1. Evaluate the run (from durable artifacts, not memory)

Reconstruct the run from the kernel artifacts above and score four axes — write to
`.handoff/work/<cycle>_evaluation.md`:

- **Friction** — wasted cycles, retries, cards that bounced `status:"blocked"`→`backlog`, places a
  crew member had to guess because an instruction was ambiguous, `hf drift` out-of-scope writes.
- **Gate quality** — did `verification-gate` catch real defects? Did any defect slip past it (caught
  later — e.g. CI red after an in-context "green")? Did it false-block correct work? A gate that both
  missed a real bug *and* false-blocked is the highest-value upgrade target.
- **Coverage** — anything left `backlog`/`blocked`, deferred, or silently capped vs. the real
  backlog; any **no-downgrade** violation (a feature stubbed/gated-out rather than completed).
- **Human walls** — every `.handoff/NEEDS-HUMAN` / manual intervention: genuine wall or an avoidable
  gap the harness could close?

## 2. Mine generalizable lessons

Root-cause each friction point to the **class** of problem, not the instance. The test: *would this
lesson help a future cycle on a different card?* If it only helps the exact card you just saw, it's
overfit — re-generalize or drop it.

> Bad (overfit): "implementer left `storage.rs:purge` half-wired." Good (general): "implementers drop
> error branches under cross-boundary pressure → the feature-build skill needs a per-branch no-stub
> checklist, and the orchestrator should size cards to fit one cycle."

A lesson seen **once** is *noted* in `LESSONS.md`; the **second** recurrence of the same class →
upgrade now (the recurrence counter is why the ledger is append-only across runs).

## 3. Route each lesson to a target

| Lesson type | Target | Typical edit |
|-------------|--------|--------------|
| Output too shallow / wrong | the crew member's **skill** (usually `feature-build`) | add a criterion, a worked example, a checklist |
| Missing capability / fuzzy role | the **agent** def (`.claude/agents/*.md`) | sharpen the role; or *propose* a new agent |
| Wrong phase order / missing step / dead data path | the **orchestrator** (`prompt-loop` SKILL.md) | reorder / add a step / fix the bus |
| Skill didn't trigger when it should have | the skill **description** | add the missing trigger phrasing |
| Same helper written by hand repeatedly | the skill's **`scripts/`** | bundle it once |

## 4. Apply or propose — fail-closed

**Auto-apply** only *low-risk, in-scope* edits: tighten an instruction, add an example/trigger/
checklist item, bundle a repeated script, fix a stale reference.

**Propose for owner approval** (write `.handoff/work/<cycle>_proposed-upgrades.md`, don't apply):
add/remove/merge an agent, reorder phases, change team composition, or touch any gate/guard
(`verification-gate`, the both-config gates, DONE criteria, apply policy, P7/kernel rules).

Hard rules:
- **Never weaken a gate.** You may only strengthen QA/verify/DONE criteria. "Loosen the check so
  cycles pass" is a defect; refuse it and record why. Includes no-downgrade, `-D warnings`,
  `#![forbid(unsafe_code)]`, and the default-AND-`--all-features` requirement.
- **Scope law.** Upgrade only the harness that ran (prompt-loop). Cross-asset lessons (the `hf`
  kernel; the canonical `harness_hub` copy of this skill) are *proposed* to that owner, never
  force-applied here.
- **Standard flow.** Every applied change goes feature branch → PR → auto-merge with a `CLAUDE.md`
  change-history row. Never an uncommitted live mutation; never mid-cycle.
- **Smaller is safer.** The minimal upgrade that fixes the root cause beats a rewrite.

## 5. Record (durable memory)

- Append every lesson to repo-root **`LESSONS.md`** — append-only; bump the recurrence counter; set
  status `noted` / `applied` / `proposed`.
- Add a `CLAUDE.md` change-history row for each applied upgrade (date · change · target · reason=the lesson).

## Lessons ledger row format (`LESSONS.md`)

```
| <date> | prompt-loop | <lesson (class, generalized)> | <evidence: PHTASK/cycle/commit> | <recurrence> | <routed-to> | <status> |
```
