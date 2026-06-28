# Fleet Convergence — Planning Engineer Loop, FIRST RUN

> Invocation brief. Point the planning-engineer harness at this file:
> `/plan-loop` (or `/harness:plan-loop`) → "read and execute `prompt_hub/prompts/fleet-convergence-first-run.md`".
> Single-cycle alternative for a more cautious start: `/planning-engineer` with the same brief.

---

## 1. What this run is (the frame — read first)

`meta` is **one system mid-assembly**, not a bag of independent repos. Every member repo is an **organ
converging on a shared north-star**:

- **rusty-idd** — the **intent-driven control plane** (the *why/what* that drives work).
- **weave** — the **communication layer** (agent-to-agent / background transport; the nervous system).
- **dual-model use** (Codex foreground + Opus-via-weave background) — an **accuracy strategy**, not a
  hosting accident.
- the **axes** (persistent memory/vector intelligence, constant auto-research, rules/policy/agent-org +
  A2A, Rust+Lua runtime, distributed compute across owner hardware, multi-vendor local+cloud mesh) —
  the **destination** every repo is heading toward.

**The planning loop is the convergence engine.** Its job is not "plan an arbitrary repo." It is:
**plan each repo's path INTO the one fabric**, against the shared north-star, continuously. The loop
plans the system that contains the loop — it is self-hosting. Open architectural questions (e.g. where
the shared north-star artifact should live) are the loop's **deliverables**, not its blockers — record
them as findings with a recommendation; do **not** pre-answer them in config.

## 2. Where to run from (the weave question — resolved)

**Run from `envctl`** (`/home/drdave/Desktop/meta/envctl`), with `META_ROOT=/home/drdave/Desktop/meta`
exported. **Do NOT run from the `weave` directory.** Why:

- The harness is **ejected into envctl** (`.claude/` + `.agents/` + `.codex/`) — that is where the
  skills, the `plan-weave-dispatch.sh` / `plan-artifact-gate.sh` runtime gates (repo-root-relative),
  the contract tests, and `reap-worktrees.sh` all resolve.
- `weave` is the **transport the loop USES, not where it lives**. The dispatcher already finds it via
  `WEAVE_BIN` → `PATH` → `$META_ROOT/weave/target/{release,debug}/weave` — cwd-independent. Running
  *from* weave would mis-place plan-state residency (`.handoff/loop/plan/` would land inside the
  comms-layer repo) and break the repo-root-relative script paths.
- envctl is the fleet's environment manager and a first-class meta peer; from it, the fleet index
  (`meta/.meta.yaml`) is the parent, readable via `META_ROOT`.
- The harness's own weave orchestrator name is literally `envctl-plan-orchestrator` — envctl is the
  designed run-from.

If anything about this placement turns out wrong, **record it as a governance + filesystem-layout
finding** this run and propose the correction (dogfood the decision); do not silently relocate.

## 3. First actions (session start)

1. Confirm `cwd = meta/envctl` and `META_ROOT=/home/drdave/Desktop/meta` is exported (so weave + the
   fleet index resolve).
2. **Reap stale worktrees/branches** (mandatory at session start):
   `bash scripts/reap-worktrees.sh` (preview) → `--apply`. merged ≠ clean.
3. Confirm a clean worktree on the loop branch; confirm `weave` resolves (or note the transport gap).

## 4. North-star binding

- Treat **`prompt_hub/prompts/planning-engineer-loop.prompt.yml`** as the upstream **north-star intent**.
  When the harness and the prompt differ, **preserve the stricter requirement**; never silently
  downgrade the prompt's intent.
- Read **`meta/.meta.yaml`** as the fleet index to derive the target backlog (all member repos).
- If **no shared, fleet-level north-star artifact** exists that *every* repo can read (vs. it living
  only inside envctl's skill prose), then the **first deliverable** is to PLAN where it should live —
  a `findings/` recommendation across the filesystem-layout + governance + rules-policy-org axes. Also
  resolve the **`harness_hub` audience** question (internal-only vs shareable marketplace) as a finding,
  because it determines whether the north-star binds in the skill or in a meta-level layer the skill
  reads. **Bind-to-north-star-as-data > hardcode-as-prose.**

## 5. Scope for cycle 1 (keep it observable)

- **Cap the first run:** `cycle_budget = 1`, `wrap_every = 1` — run **one full planning cycle**, then
  produce a consolidated summary and **HAND OFF**. Do **not** continue unattended on the first run; the
  owner reviews before the loop runs free.
- **Auto-derive** the fleet backlog from `.meta.yaml`, then **prioritize `rusty-idd`** as the first
  target (the north-star expects rusty-idd to surface first as the path into the Forge/IDD loop). If
  auto-derivation misses it, seed `rusty-idd` explicitly and record that as a governance/config finding.
- Run the full crew on the first target: cartographer ‖ trend-researcher → analysts (incl. the
  extended axes + `test-coverage`) → verifiers (gate) → architect → `evolution-steward` self-eval.

## 6. Required outputs of this run (under `.handoff/loop/plan/`)

1. **Fleet north-star map** — the member repos as organs, current integration state, and the gap each
   has to the fabric (cross-repo edges where known).
2. **`rusty-idd` convergence report** — current-state → gap-to-fabric → **sequenced upgrade path**
   (each upgrade row carrying axis · target-surface · evidence · blast · effort · risk-tier · the P8
   acceptance test it maps to · reversibility), with ASCII current/target/control-plane diagrams.
3. **Decision-findings** (recommendations for owner approval, NOT applied):
   - where the shared north-star artifact lives + how repos bind to it as data;
   - run-from / plan-state residency / weave transport (validate §2);
   - harness_hub audience (internal vs shareable).
4. **Per-axis findings** for the first target, including the mandatory architecture-loop axes
   (`memory-vector-intelligence`, `autoresearch`, `rules-policy-org`, `distributed-compute`). For a
   target where an axis is genuinely N/A, the finding must say **"N/A — <why>"**: the gate requires an
   **answer**, not necessarily work. A missing axis finding blocks DONE.
5. **TDD RED-suite evidence** (P8) for accepted upgrades — additive tests only, `tests-ran > 0`,
   traceability matrix, Feature-Forge GREEN handoff.
6. **Self-eval + LESSONS** for the cycle, and the **resume pointer** for the next session.

## 7. Standing laws (non-negotiable)

- **Read-only on product code.** The loop writes only `.handoff/loop/plan/` artifacts + docs. The one
  permitted mutation is **additive RED tests** (P8) — never product code, never weakening a gate.
- **Fail-closed.** A green exit / empty result / missing file is a finding to investigate, never a
  pass. Every claim cites positive evidence (`file:line` / graph-query row / dated URL).
- **Owner walls → NEEDS-HUMAN.** Physical / account / irreversible / scope-expanding actions are
  surfaced, never silently performed.
- **Strict upgrade only.** No downgrades, no destructive resets; do not remove a legacy tool until a
  Rust/meta-native replacement is installed, configured, and **parity-proven** (e.g. shimmy+ruvllm
  replace ollama but ollama stays until the swap is proven; bun not pnpm; clang/llvm-21 load-bearing).
- **Dual-model accuracy strategy.** Heavy research / code-mapping / governance scans run as
  **background lanes** via the harness's background-agent launch contract — **weave → Opus** when
  running from Codex, or direct Opus sub-agents (`run_in_background`) when the foreground is Claude. If
  an Opus worker cannot be obtained where the contract requires one, **fail closed** with a
  provider/transport gap; do not silently drop to a weaker model.
- **Per-cycle self-eval + self-upgrade**, fail-closed, at the cycle boundary (never mid-cycle); only
  ever strengthen the verify/completeness/DONE gate.

## 8. After the run

Produce the consolidated summary, write the resume pointer + (if budget hit) the cold-start HANDOFF,
and surface the three decision-findings for owner review. Then **stop** — await owner approval before
unattended continuation.
