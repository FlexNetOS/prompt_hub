# _workspace/HANDOFF.md — DEPRECATED

> ⛔ **DEPRECATED — migrated to `.handoff/` (2026-06-13).**
>
> The prompt-loop construction crew no longer keeps state here. The single source of
> truth is now the Continuity Ledger Kernel layer at `prompt_hub/.handoff/`:
>
> | Old (`_workspace/`) | New (`.handoff/`) |
> |---|---|
> | `backlog.md` | `tasks/PHTASK-NNNN.task.json` (40 cards) |
> | `HANDOFF.md` | `packets/latest.md` (derived: `hf fleet render prompt_hub`) |
> | `loop_state.md` | `active.md` + the FLEET ledger (`meta/.handoff`) |
> | `DONE`/`STOP` sentinels | card `status` + `hf` verbs |
>
> Full original content is archived verbatim under `.handoff/history/_workspace-archive/`.
> Cold-start: read `.handoff/context/capsule.json`, then run `hf resume`.

## Legacy note preserved from pre-migration handoff

The previous `_workspace/HANDOFF.md` guidance (resume command, baseline checks, cycle
history, and findings) was superseded by the `.handoff/` kernel and is retained in
the archive path above for forensic continuity.
