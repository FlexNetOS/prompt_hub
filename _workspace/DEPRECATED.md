# _workspace/ — DEPRECATED (migrated to .handoff/ on 2026-06-13)

This directory was the prompt-loop construction crew's durable state store
(`backlog.md` + `loop_state.md` + `HANDOFF.md` + sentinels). It has been migrated to
the canonical **Continuity Ledger Kernel** layer at `prompt_hub/.handoff/` per the
owner directive "adopt the new handoff system; migrate the deprecated backlogs and
handoffs; no downgrades, upgrade only."

- **Backlog** → `.handoff/tasks/PHTASK-NNNN.task.json` (40 `handoff.task.v1` cards:
  27 done, 12 backlog, 1 blocked), each with an `hf`-identical blake3 `intent_lock`.
- **Resume packet** → `.handoff/packets/latest.md` (`handoff.packet.v2`), derived via
  `hf fleet render prompt_hub` — never hand-edited.
- **Loop counters/state** → `.handoff/active.md` + witnessed events in the FLEET
  ledger (`meta/.handoff/ledger.db`).
- **Provenance** → every original file is preserved verbatim under
  `.handoff/history/_workspace-archive/`, alongside the migration generator
  (`.handoff/history/generate_cards.py`).

Do not add new state here. Use `hf resume` / `hf claim` / `hf checkpoint` and the cards.
