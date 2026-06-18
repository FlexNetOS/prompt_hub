# ADR-0001 — Adopt the Continuity Ledger Kernel (`hf` + `.handoff`) for prompt_hub

- **Status:** Accepted
- **Date:** 2026-06-13
- **Deciders:** owner directive ("adopt the new handoff system from meta/handoff; migrate
  the deprecated backlogs and handoffs to `.handoff`; no downgrades, upgrade only")
- **Context refs:** `meta/handoff/FLEET_GUIDE.md`, handoff ADR-0003 (planning↔execution seam),
  handoff ADR-0004 §3 (two-ledger residency / git-text-only members)

## Context

prompt_hub's autonomous "construction crew" loop previously kept its durable state in
`_workspace/{backlog.md, loop_state.md, HANDOFF.md}` + ad-hoc sentinels. That store was
bespoke, unversioned against any schema, and invisible to the fleet. The FlexNetOS meta
workspace standardized on the **Continuity Ledger Kernel** (`hf` + `.handoff/`): typed task
cards, a derived resume packet, witnessed events, and a lifecycle-hook substrate.

## Decision

Adopt the kernel layout wholesale for prompt_hub as a **Tier-B FLEET member**, copying the
full reference `.handoff/` directory structure from `meta/handoff/.handoff` and adapting the
repo-specific content:

| Component | Adoption |
|-----------|----------|
| `context/capsule.json` | prompt_hub identity + northstar + `hf resume` next_command |
| `tasks/PHTASK-*.task.json` | the migrated backlog — 40 `handoff.task.v1` cards (27 done, 12 backlog, 1 blocked), each with an `hf`-identical blake3 `intent_lock` |
| `packets/latest.md` | `handoff.packet.v2`, **derived** by `hf fleet render prompt_hub` |
| `active.md` | one-line next-card pointer + done count |
| `policy.toml` | `handoff.policy.v1` — adapted: origin `FlexNetOS/prompt_hub`, trunk `main`, squash auto-merge, worktree prefix `prompt-` |
| `policies/rules.toml` | `handoff.policy.rules.v1` — fail-closed write/network/dependency gates, protected-file guard (incl. `.handoff/**` + the loop skills + Cargo manifests) |
| `hooks/{hooks.toml,loop-entry.sh,session-end.sh}` | `handoff.hooks.v1` lifecycle substrate, adapted to the member model (render via `hf fleet render`, invoke the `prompt-loop` skill) |
| `skills/session-resume.skill.md` | member resume procedure |
| `history/` | the deprecated `_workspace/` archived verbatim + the migration generator |

## Member model (the key constraint — ADR-0004 §3)

prompt_hub has **no local `ledger.db`**. `.handoff/` is **git-committed text only**. Witnessed
events live in the **FLEET ledger** at `meta/.handoff/ledger.db`; the packet is compiled
centrally by `hf fleet render prompt_hub` (run from the meta root). We therefore deliberately did
**not** copy the kernel's `ledger.db` (binary, and its own history) nor its `fleet/` census, and
do **not** run `hf init`/`hf seed` here (those would create a forbidden per-repo ledger).
`.handoff/.gitignore` enforces this (ignores `ledger.db`, the control sentinels, `work/`, logs).

## Consequences

- **+** One cold-start onboarding for any agent: read `context/capsule.json`, run `hf resume`.
- **+** The backlog is now typed, schema-validated, intent-locked, and fleet-visible
  (`hf fleet status` shows prompt_hub: 40 cards).
- **+** A lifecycle-hook substrate exists for fully unattended operation (wired in
  `.claude/settings.json` SessionStart/SessionEnd → `.handoff/hooks/`).
- **−** Some hook verbs the contract names (`hf policy check-*`, `hf drift`) are kernel features
  still maturing; those hooks are advisory (`fail_mode = warn`) until available, and the
  `prompt-loop` skill remains the operational driver.
- The `prompt-loop`/`session-relay` skills + `ralph-prompt.sh` runner were rewired to
  `.handoff` + `hf` (see CLAUDE.md change history 2026-06-13).

## Provenance / no downgrade

Every original `_workspace/` artifact is archived verbatim under
`.handoff/history/_workspace-archive/`, with the reproducible card generator
`.handoff/history/generate_cards.py`. Nothing from the old system was lost.
