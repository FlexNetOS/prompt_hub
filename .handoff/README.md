# .handoff — prompt_hub continuity layer (Tier-B, git-text-only)

This repo is a member of the FlexNetOS meta workspace. This directory is its
**Continuity Ledger Kernel** layer (`hf` + `.handoff`; design: handoff ADR-0003 +
ADR-0004 §3; FLEET_GUIDE at `meta/handoff/FLEET_GUIDE.md`).

> Cold-start onboarding: read `context/capsule.json` + this README, then run `hf resume`.

## Layout
- `context/capsule.json` (`handoff.context_capsule.v1`) — who this repo is and what's next.
- `tasks/PHTASK-NNNN.task.json` (`handoff.task.v1`) — the execution cards. **This is the
  backlog**, migrated from the deprecated `_workspace/backlog.md`. Each card carries a
  blake3 `intent_lock` (the drift-sentinel anchor, `hf`-identical).
- `packets/latest.md` (`handoff.packet.v2`) — the resume packet. **Derived** — regenerate with
  `hf fleet render prompt_hub` (run from the meta root), never hand-edit.
- `active.md` — one-line pointer to the next card + done count.
- `history/` — provenance: the archived `_workspace/` artifacts (full backlog, every HANDOFF,
  loop_state, cycle notes) + the migration generator. Nothing from the old system was lost.

## Rules (ADR-0004 §3)
- **Git-committed TEXT ONLY** — never a `ledger.db`, never binary state in this directory.
  Witnessed events live in the **FLEET ledger** at `meta/.handoff/ledger.db` (run `hf` from
  the meta root). `hf fleet status` flags any stray per-repo `ledger.db` as a P7 violation.
- **State precedence:** `Git > FLEET ledger > tasks/*.task.json > active.md > packets/latest.md`.
  Cards and packets are derived/declarative views — regenerate, don't hand-edit the packet.

## Daily flow (the `hf` verbs, run from the meta root)
```bash
hf resume                       # where am I? what's next?
hf fleet render prompt_hub      # recompile packets/latest.md from cards + FLEET ledger
hf claim PHTASK-0028            # reserve a card (no edit without a claim)
hf checkpoint PHTASK-0028 "…"   # witness progress
hf drift                        # am I in scope / intent-locked?
hf handoff                      # render the next-session packet
```

## Migration note (2026-06-13)
The autonomous **prompt-loop** construction crew previously kept its durable state in
`_workspace/{backlog.md, loop_state.md, HANDOFF.md}`. That system is **deprecated**; its
content was migrated here (40 cards: 27 done, 12 backlog, 1 blocked) with no downgrade.
The old files are preserved under `history/` and stubbed with deprecation pointers.
