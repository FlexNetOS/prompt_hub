# Environment-State Convergence Template

Operating doctrine for any task that touches **environment state** in a declaratively-managed
workspace (e.g. the meta workspace, where `envctl` is the environment manager): host config, the
agent env (`.claude`/`.codex`), a plugin/marketplace cache, dotfiles, daemons, toolchains, or any
path under a real home directory.

> **The framing — ask this BEFORE acting:**
> "This is environment state — what does the manager declare for it, and is reality converged to that?"

The full reusable prompt (system message + situational user slot + worked examples) lives at
[`prompts/env-state-convergence.prompt.yml`](../../prompts/env-state-convergence.prompt.yml). Use
this template as the quick doctrine reference; use the `.prompt.yml` to actually run the check.

## Core model
The live runtime (real home dir, running daemon, installed binary, plugin cache) is a **managed
output**, not a source of truth. The source lives in-repo: the declarative overlay (e.g. a `home/`
overlay), the agent-env config + lock, and manifest components. Reality is **materialized** from
those by the manager's verbs.

## Two instincts that are both WRONG
1. **Hand-editing the runtime** (`rm` a stale cache, refetch, `cp` a binary, edit a dotfile in
   place) — drifts reality from the declared state; does not survive a reset.
2. **"That's host config, hands off / you go run it"** — converging env state *is* the manager's
   entire purpose; treating it as off-limits abdicates the job.

## The discipline: detect → declare → sync → lock
1. **Detect** — treat the symptom as drift; use the manager's drift/health verbs (`doctor`,
   `auto-detect`), not a one-shot UI refresh.
2. **Declare** — find/create the in-repo declarative source; fix the **source**, not the runtime.
3. **Sync** — converge via the manager's verbs (`install`/`auto-fix`), **dry-run by default**,
   explicit `--apply` to mutate, **fail-closed**.
4. **Lock** — update the lock so the reproducible state reflects the change and CI can gate it.

## Means vs outcome
Even when the **outcome** is authorized ("fix the cache", "reopen the vault", "restart the
daemon"), the **means** must stay in-scope: a manager verb, or hand the exact steps to the owner.
Never raw-mutate the runtime. **Outcome-authorized ≠ means-authorized.**

## Fields
- **Situation**: {{env_state_situation}}
- **Verdict**: {{verdict}} _(real drift vs stale/cosmetic — check the declared state first)_
- **Declared source**: {{declared_source}}
- **Convergence plan**: {{convergence_plan}} _(detect → declare → sync → lock, with the verbs)_
- **Means check**: {{means_check}} _(every mutating step is a manager verb or owner-handed)_
