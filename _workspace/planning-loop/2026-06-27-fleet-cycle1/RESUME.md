# Resume pointer — meta-architecture-integration-loop · cycle 2026-06-27-fleet-cycle1

**Prompt:** `prompt_hub/prompts/meta-architecture-integration-loop.prompt.yml`
**Deliverable (durable):** `prompt_hub/docs/plans/meta-fleet-integration.md` (full-fleet integration design).
**Baseline extended:** `prompt_hub/docs/plans/lifeos-meta-front-door.md` (rusty-idd `5a55284`).

## What this cycle did (I0→I10)
- I0/I1: reaper applied (no-op, dirty worktrees protected); fleet enumerated (~70 `.meta.yaml` members).
- I2/I3: 5 read-only Opus background agents mapped the fleet in clusters (spine / envctl / agent-substrate /
  front-door+intent+harness / inference+automation+external). All source-cited, fail-closed.
- I4–I7: ASCII current+target diagrams; extended ownership matrix; 22-seam catalog (S1–S22); front-door pattern;
  gap→upgrade table (G1–G7); P0–P2 roadmap. → all in `docs/plans/meta-fleet-integration.md`.
- I8: architecture cycle → seams design-only; acceptance criteria authored, RED tests deferred to per-seam Feature-Forge
  cycles (tests-ran=0, honest reason recorded).
- I9: evolution scorecard → `evaluation.md`.

## Owner decisions LANDED 2026-06-27 (resolve D-G1 & D-G3)
- **D-G1 inference authority = `cellm`** (`github.com/FlexNetOS/cellm`; Rust mobile-native serving, paged-KV, Metal/Vulkan,
  <512MB). teri: ruvllm > shimmy, but cellm > both. **cellm is NOT a `.meta.yaml` member yet** → adopt + envctl component;
  keep ollama/shimmy/ruvllm until cellm parity-proven (strict-upgrade).
- **D-G3 union merge = 3-phase Frankenstein merge** (general doctrine for any 2 Rust repos): (1) combine as-is →
  (2) path/data-flow organization (consolidate dot-dirs, e.g. `.handoff/.idd`, `.handoff/.kb`) → (3) per-symbol
  endpoint-to-endpoint merge, every change gated by an output-constant-or-better parity test. grit = coordination only.

## Next cycle should
1. **G1 (cleanest single-repo start):** `meta add-repo github.com/FlexNetOS/cellm` + envctl inference component
   (pinned, ollama swap-parity gate). Author its RED suite (differential vs ollama).
2. Highest single lever: **G4/S14** — build the harness_hub-markdown → `WorkflowDefinition` parser so packaged
   harnesses execute on the `harness-agent-rs` Rust DAG runtime (closes the intent→execution loop).
3. **G3:** apply the Frankenstein merge to the handoff⊕rusty-idd union (parity-gated, phase by phase).
4. APPLY-tier quick wins (separate PRs): G5 mint prod path (frozen contract exists), G7 hygiene items.
5. **Index the fleet code graph** (`git kb code index`) so the next map gets real call-graph edges, not dependency-edge fallback.

## Remaining owner walls (NEEDS-HUMAN, surfaced not performed)
- Disposition of 4 empty repos (`flexnetos_wiki/brain`, `my-wiki`, `assets`) + empty hubs (`hooks_hub`, `flow_hub`).
- kasetto canonical home (method now decided = Frankenstein merge; the keep-which choice is still owner's).
