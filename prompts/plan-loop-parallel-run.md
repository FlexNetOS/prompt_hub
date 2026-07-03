# Plan Loop — PARALLEL run (one instance per target, from envctl)

> Reusable invocation brief. Point the planning-engineer harness at this file, with a target:
> `/plan-loop` (or `/harness:plan-loop`) → "read and execute `prompt_hub/prompts/plan-loop-parallel-run.md`, target=<repo-slug>".
> Single-cycle alt: `/planning-engineer` with the same brief. To plan many repos at once, launch N
> sessions, each with a different `target=` (see §8 Fan-out). This is the parallel-safe sibling of
> `fleet-convergence-first-run.md` — same frame + laws, isolated per instance.

---

## 1. Frame (read first — same as the convergence loop)

`meta` is **one system mid-assembly**, not a bag of repos. Every member is an **organ converging on a
shared north-star** (owner-confirmed): the **north-star lives at `$META_ROOT` + handoff**, and the goal
is the **handoff + rusty-idd union** (one continuity+intent control plane). Organs: **rusty-idd** =
intent control plane (why/what); **handoff** = continuity kernel (witnessed ledger + real-teeth gates);
**harness_hub** = the Front-Door **interpreter** (transforms user intent → model-ready language);
**weave** = transport (A2A nervous system, a *distinct* plane from handoff's witnessed receipts);
**dual-model** (Codex fg + Opus bg) = accuracy strategy. The loop plans each repo's path INTO that one
fabric. Open architecture questions are **findings with a recommendation**, never pre-answered in config.

## 2. What "parallel" means here
One **instance plans exactly one target repo**, fully isolated, and opens its own PR. Many instances run
concurrently across different targets. Isolation rests on three things: a **weave lease** (so two
instances never claim the same target), an **own git worktree + branch** (so the shared loop-state files
never clobber each other at runtime), and **per-target artifact namespacing** (already the loop's layout).

## 3. Where to run from (resolved)
**Run from `envctl`** (`/home/drdave/Desktop/meta/envctl`) with `META_ROOT=/home/drdave/Desktop/meta`
exported. The harness is ejected there; the repo-root-relative scripts (`scripts/plan-artifact-gate.sh`,
`scripts/plan-weave-dispatch.sh`, `scripts/reap-worktrees.sh`) and `.handoff/loop/plan/` state resolve
there; weave is found via `WEAVE_BIN` → PATH → `$META_ROOT/weave/target/{release,debug}/weave`
(cwd-independent). Do **not** run from the target repo.

## 4. Args
- `target=<repo-slug>` — the ONE repo this instance plans (a member of `meta/.meta.yaml`). Required for
  owner-assigned parallel.
- If `target=` is omitted → **auto-claim mode**: read `.handoff/loop/plan/graph/target-dag.md`, pick the
  **top unclaimed node in the ready-set** (deps satisfied, status not done/blocked/SUPERVISED), and claim
  it (§5). If the ready-set is empty or every ready node is already leased → report and STOP.
- `run_id=<id>` — optional; defaults to `plan-<target>-<shortdate>`. Used as the lease holder identity.

## 5. First actions (session start)
1. Confirm `cwd = meta/envctl` and `META_ROOT` is exported. Confirm `weave` resolves (or note the gap).
2. **Reap** (mandatory): `bash scripts/reap-worktrees.sh` (preview) → `--apply`. It skips worktrees with
   uncommitted `.handoff` and protects master/develop/current — safe under parallel instances.
3. **Claim the target (prevents duplicate work — reuse the weave lease):**
   `HF_LEASE_HOLDER="<run_id>" weave lease reserve --resource "plan:claim:<target>" --ttl 1800 --note "plan-loop"`
   - Held by a **different** holder → this target is taken: in auto-claim mode pick the next ready node;
     in explicit mode report "target leased by <holder>" and STOP (do not double-plan).
   - **Same** holder re-reserving = heartbeat (extend if the cycle runs long).
   - weave unavailable → the lease **degrades to ledger-only** with a visible warning (offline/CI); the
     resource key is slash-free (`plan:claim:<target>`) for weave's exact-match detection. Never silent.
4. **Own worktree (the isolation):** create `meta/.worktrees/plan-<target>/envctl` on branch
   `plan/loop-<target>` off **fresh `origin/master`**. All work happens there; nothing touches another
   instance's branch or the union loop branch.

## 6. Run ONE capped cycle on the target
`cycle_budget=1`, `wrap_every=1` — one full planning cycle on `<target>`, then PR + HAND OFF.
Run the full crew (dual-model: foreground Claude → direct Opus sub-agents `run_in_background`; foreground
Codex → weave → Opus). **Author gate-named artifacts from the start** (lesson L1 — no rename round-trips):
- **cartographer** ‖ **trend-researcher** (90-day window; `Tool-currency & advisories` + `Sources` headers;
  `research/sources-<T>.jsonl` with the required keys) → graph: `graph/<T>.{symbols,callgraph,metrics}.json`
  + `graph/<T>.{graph,diff}.md` + `reports/codemap-<T>.md`; and the global `graph/target-dag.{json,md}`
  (a node per `targets.md` slug).
- **analysts + the 8 axis auditors** → `findings/{governance-config,filesystem-layout,test-strategy,
  memory-vector-intelligence,autoresearch,rules-policy-org,distributed-compute,prompt-architecture}-<T>.md`
  (N/A axes must still answer "N/A — <why>"; never the literal words TODO/TBD/"placeholder evidence"/
  "citation needed" — the gate rejects them).
- **verifier** (gate) → append a `## <T>` section to `findings/verdicts.md` (each row: VERDICT → CONFIRMED/
  QUALIFIED/REFUTED/INCONCLUSIVE; ≥1 CONFIRMED/QUALIFIED; every UPGRADE carries a feasibility verdict).
  Run empirical experiments where a claim is checkable (e.g. a standalone build). Reconcile
  `dimensions.md` fail-closed (flip `[x]` only where a verdict covers it; leave `[~]` otherwise).
- **architect** → `reports/<T>-plan.md` with the gate's section markers (Verdict, ASCII architecture,
  Sequenced upgrade, Tool-evaluation, Governance, Filesystem layout, Memory/vector, Auto-research,
  Rules/policy, Distributed compute, Test Strategy, Prompt-architecture, Risk policy, Confidence) +
  ROADMAP/ADR drafts (DRAFTS in the plan dir; never written into the target repo's tree — owner-wall).
  Append a `## <T>` section to the global `risk-policy.md`; ensure `agent-backend-matrix.md` +
  `agent-interop.md` exist. `reports/agent-run-ledger-<T>.md` (markers: agent run ledger · lane · model ·
  artifact).
- **evolution-steward** → append a `## <T>` section to `evaluation.md` (marker: scorecard/self-eval/
  evolution) + `LESSONS.md` + `proposed-upgrades.md` (PROPOSE-only; never weaken a gate).
- **test-strategist** → additive RED tests in an ISOLATED worktree of the TARGET repo
  (`meta/.worktrees/plan-<target>-red/<target>`, off the target's PR-base branch), build+run them
  (`tests-ran > 0`, RED for the right reason), commit on a `plan/<target>-red-tests` branch; record
  `tests-ran: N` + `traceability` + `FF test-build spec` in `findings/test-strategy-<T>.md`.

## 7. Per-target write discipline (keeps parallel PRs from colliding)
Edit **shared** files for THIS target only — append under a `## <target>` header, don't rewrite others':
- `targets.md`: flip only this target's row (`[ ]`→`[~]`/`[x]`/`[!]`); never touch other rows. (Active
  rows must be one kebab-case slug each — the gate's parser rejects snake_case/multi-slug/prose rows;
  keep snake_case fleet members in the `#`-commented backlog.)
- `dimensions.md`: append this target's `## <target>` dimension block.
- `findings/verdicts.md`, `evaluation.md`, `risk-policy.md`: append a `## <target>` section.
- `loop_state.md`: per-branch — set `planning_target=<target>`, counters for THIS instance; a later
  wrap-up reconciles across instances (state precedence: **Git > markdown**; the hf witnessed ledger,
  when present, outranks the markdown).
Per-target artifacts (`graph/<T>.*`, `findings/<axis>-<T>.md`, `reports/*-<T>.md`, `research/<T>.*`,
`research/sources-<T>.jsonl`) never collide — different filenames.

## 8. Gate, ship, release (each instance)
1. Validate: temporarily mark `<target>` `[x]` in `targets.md`, run
   `bash scripts/plan-artifact-gate.sh .handoff/loop/plan` → require `PASS`, then restore the honest
   status (`[~]` planned-with-gaps if any dimension stayed `[~]`; `[x]` only if fully verified). Never
   write the `DONE` sentinel for a single target — `DONE` = the whole backlog planned + verified.
2. Commit the plan artifacts on `plan/loop-<target>`; **push; open an envctl PR; arm auto-merge**
   (`gh pr merge <n> --auto --squash`). Push the target's RED-test branch and open its PR on the TARGET
   repo (mind each repo's PR-base rule — e.g. handoff/weave PRs target **develop**, not master; a
   guard auto-closes master PRs). `icm store` the outcome.
3. **Release the lease:** `HF_LEASE_HOLDER="<run_id>" weave lease release --resource "plan:claim:<target>"`.
4. HAND OFF — do not auto-continue to another target in the same instance (one target per instance).

## 9. Fan-out (launching the wave) + reconcile
- **Launch N instances**, each its own session, each with a distinct `target=` (or all in auto-claim
  mode — the lease guarantees no two grab the same node). Recommended first wave: independent organs with
  no cross-deps in the `target-dag` ready-set.
- **Collision note (known rough edge):** concurrent envctl PRs can still conflict on `loop_state.md` /
  `targets.md` / `graph/target-dag.{json,md}`. Mitigations: the per-target write discipline (§7) keeps diffs region-isolated and
  mostly auto-mergeable; merge the PRs sequentially if needed; a follow-up loop wrap-up reconciles
  `targets.md`/`loop_state.md` to status-truth. (Proposed harness upgrade: namespace run state under
  `.handoff/loop/plan/runs/<target>/` to remove the shared-file conflict entirely — see
  `proposed-upgrades.md`; not required for correctness.)
- **End of wave:** run the batch boundary — `reap-worktrees.sh --apply`, a `session-relay-wrap-up`
  reconcile over `targets.md`, and surface `proposed-upgrades.md` for the owner.

## 10. Standing laws (non-negotiable — same as the convergence loop)
- **Read-only on product code.** The loop writes only `.handoff/loop/plan/` artifacts + DRAFT docs. The
  one permitted mutation is **additive RED tests** (P8), in an isolated target worktree.
- **Fail-closed.** A green exit / empty result / missing file is a finding, never a pass. Every claim
  cites positive evidence (`file:line` / graph row / dated URL).
- **Owner walls → NEEDS-HUMAN.** Physical / account / irreversible / scope-expanding actions are
  surfaced, never silently performed.
- **Strict upgrade only.** No downgrades, no destructive resets; never remove a legacy tool until a
  Rust/meta-native replacement is installed, configured, and parity-proven.
- **Dual-model accuracy strategy**, **per-cycle self-eval + self-upgrade** at the cycle boundary, and
  **only ever strengthen** the verify/completeness/DONE gate — never weaken it.
- **Lease discipline.** Always claim before planning and release after PR; never double-plan a leased
  target; degrade visibly (never silently) when weave is absent.
