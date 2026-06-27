# LifeOS Meta Front-Door — Integration Plan

> **➤ Extended 2026-06-27 by the full-fleet cycle → [`meta-fleet-integration.md`](./meta-fleet-integration.md).**
> That cycle re-validated this baseline against current HEAD with 5 source-citing agents and corrects it:
> (1) LifeOS is a **real multi-crate app with a durable AI runtime** (not a skeleton); (2) the ruvector seam is **already
> chosen** (MCP-REST mirror), superseding `[V2]`'s "not yet selected"; (3) network-control composes **lane only** (NOT
> ruvector — corrects `[N1]`); (4) the baseline omits the **harness execution layer** (`harness-agent-rs` Rust DAG runtime);
> (5) the prompt_hub intent-boundary "ADR-0007" citation mis-points (prompt_hub's ADR-0007 is the plugin system). The
> two-front-door + handoff⊕rusty-idd-union verdicts are **confirmed in source**. Read the fleet doc for the extended matrix.

> **Status:** synthesized plan (decision-grade, evidence-backed)
> **Source of truth:** `rusty-idd` plan-loop run on branch `plan/lifeos-meta-front-door`
> @ commit `5a55284` (3 cycles complete). This doc distills its committed
> `.handoff/loop/plan/` artifacts into one consumable plan for prompt_hub.
> **Synthesized by:** envctl session, 2026-06-27. Coordination: weave `#177` (envctl →
> rusty-idd) received a live reply `#178` — **corrections folded in below** (front-door is
> two-layered; handoff⊕rusty-idd union; grit unfit as union merge engine; Odysseus endorsed).
> **Provenance of every claim:** the `[L#] [E#] [W#] …` source keys below resolve to the
> rusty-idd report they came from and the real meta path each report cited.

---

## Verdict

**LifeOS is the owner-facing "front door" — a shell, not a monolith.** It renders status,
launches panels, accepts owner intent, and exposes safe controls. It does **not** collapse the
meta workspace into itself. Each subsystem keeps its **authoritative engine**: prompt_hub
(prompts), rusty-idd (goals/specs/tasks), envctl (installs/env state), weave (A2A/jobs/leases),
handoff (continuity), meta-ruvector (vector/memory/agent substrate), and
network-control/lane/obscura (network planes). [L1][L2][P1][R1][E1][W2][H2][V2][N1][N2][N3]

Strict-upgrade-only governs the whole integration: no downgrade, no destructive reset, no legacy
removal until a replacement is installed, configured, parity-proven, and rollback-safe.

### Two "front doors" — do not conflate (rusty-idd reply #178)

The meta vocabulary has **two** front doors; this plan keeps them distinct:

1. **UI front door** — **LifeOS**, the owner-facing shell (this doc's main subject).
2. **Intent front door** — the pipeline that turns owner intent into spec/goal lifecycle, which
   is **two-layered** (owner decision **D3**, 2026-06-26): **harness_hub** is the front-door
   *interpreter* (transforms user intent → model-ready language), and **prompt_hub** is the
   durable intent *store + boundary* (ADR-0007). Together they feed **rusty-idd**, which owns the
   OpenSpec/ADR/task/validation/manifest/PR flow; prompt_hub never owns rusty-idd's lifecycle.
   **Do not model the intent front door as prompt_hub-only.**

> **handoff ⊕ rusty-idd union (owner decision D1):** the shared north-star lives at `META_ROOT` +
> handoff, and the goal is the handoff + rusty-idd **union** — one continuity+intent control
> plane. So in the matrix below, the **handoff** and **rusty-idd** rows are *converging into a
> union*, not independent peers. (Caveat from rusty-idd's live cycle-5 grit planning: **grit is
> unfit as-is to be the union MERGE engine** — it is an advisory symbol-LOCK + git-worktree
> coordinator doing line-level `git merge`, computes per-symbol content hashes but never reads
> them. The ~95%-shared handoff/rusty-idd crate dedup needs grit as the *coordination substrate
> around* a separate symbol-level reconciliation engine, not as the reconciler itself.)

---

## End-state architecture

LifeOS = Vue 3 + Vite + Pinia + vue-router inside a Tauri 2 desktop shell (with a web build):
six addressable workspaces, global OS surfaces, a settings/profile vault, and hardware
inventory. [L1][L2] It is the aggregation/launch/control surface; the engines stay where they
are and are reached through named seams.

### Integration DAG (ASCII rendering of `graph/target-dag.md`)

```
        work-definition authority                       install/verify authority
        ─────────────────────────                       ────────────────────────
  prompt_hub ──intent/prompt──▶ rusty-idd ──ready goal/spec──▶ planning_engineer
                                    ▲                                  │
                                    │ resume state                     │ build chunks
                                    │                                  ▼
                                 handoff ◀──jobs/messages/leases── feature_forge
                                    ▲                                  │
                                    │                                  │ install/verify
                                    │                                  ▼
                                  weave ◀──MCP/API/events bridge──   envctl
                                    ▲                                  │
                  governed web      │                                  │ component status/config
                  egress            │                                  ▼
   network-control/lane/obscura ────┴──fleet/network/privacy──▶     LifeOS ──/ai panel──▶ Odysseus
                                                                       ▲
                                  meta-ruvector ──memory/vector/agent──┘
```

**Ordering rule:** install + verification authority must flow through **envctl** before LifeOS
treats a component as managed; work-definition authority must flow through
**prompt_hub → rusty-idd** before Feature Forge builds. [E1][R1][P2]

---

## Component-ownership matrix (16 components)

| Component | Owns | Outputs / APIs | Gaps / next proof |
|---|---|---|---|
| **LifeOS** | Owner UI shell, workspaces, global OS surfaces, settings/vault/hardware inventory | Tauri/Vue desktop+web UI | Durable AI chat, multimodal ingestion, tool routing, control-plane adapters [L1][L2][L3] |
| **Odysseus** | Candidate AI workspace (chat/agents/research/docs/email/calendar/local models) | Web app/API on a local port | Sandbox, pin, auth-gate, license-review (external, fast-moving) [O1–O5] |
| **prompt_hub** | Prompt source-of-truth, search, lineage, RBAC/audit, planning prompts | CLI/server/library prompts + bundles | Bind prompts to rusty-idd changes + LifeOS intent UI [P1][P2] |
| **rusty-idd** | Intent/spec/goal lifecycle, OpenSpec engine, runner, manifest/validation | `rusty-idd` CLI / OpenSpec state | Archive active change, then drive plan-loop work definitions [R1] |
| **weave** | A2A mesh, messages/asks/jobs/leases/permissions, spawn/kill | CLI + token-light MCP meta-tool + job board | Prefer Damian/job-lane for background scans; avoid token-heavy discovery [W1][W2] |
| **handoff** | Continuity ledger, claims/leases, task state, proofs, packets | ledger / events / packets / status | Project LifeOS-visible status/watch feed + exact resume packets [H1][H2][H3] |
| **envctl** | Meta-local installs, components, add-repo, locks, secrets/env path authority | CLI/GUI auto-detect/install/reset/lock/add-repo | Add Odysseus component with strict gates; expose JSON to LifeOS [E1][E2][E3] |
| **meta-ruvector** | Vector/memory/agent/runtime/gate substrate (314-crate inventory) | RVF, vector stores, rvAgent, MCP gates/brain, WASM/API | Select LifeOS memory/vector seam by trait/API, not crate-name guesses [M2][V1][V2] |
| **network-control** | Off-host fabric (Omada/switch/AP/gateway/VLAN/VPN) | `netctl` CLI/GUI JSON | LifeOS network workspace adapter + weave coordination [N1] |
| **lane** | Local HTTPS domains, tunnels, host/network plane spine | local domain proxy/tunnel CLI | Local service routing layer for LifeOS/Odysseus [N2] |
| **obscura** | Rust headless browser / web automation engine | CDP/Puppeteer/Playwright-style surface | Govern via weave/lane policy only — never raw LifeOS exposure [N3][W2] |
| **Meta CLI / canon repos** | Workspace project graph, plugins, dashboard | meta commands, plugin protocol | Keep as substrate; do not duplicate in LifeOS [M1] |
| **Hubs/repos** | template/assets/flow/harness/network/tool/database/mcp/plugin/hooks/commands/vault | curated collections | Classify before automating; unverified claims stay out [M1] |

(LifeOS, Odysseus, prompt_hub, rusty-idd, weave, handoff, envctl, meta-ruvector, network-control,
lane, obscura = the 11 engine components; Meta-CLI/canon + hubs are the substrate rows. Full
matrix incl. inputs + automation-state columns in
`rusty-idd/.handoff/loop/plan/reports/component-ownership-matrix.md`.)

---

## Integration pattern (6 steps)

1. LifeOS owns navigation, auth UX, workspace aggregation, owner settings, and component status. [L1][L2]
2. Odysseus is mounted as a sandboxed `/ai` workspace panel **first** — not source-merged. [O1][O4][O5]
3. envctl owns install, pinning, local bind, data directory, health checks, rollback, secrets wiring. [E1][E3]
4. weave/handoff capture events, jobs, continuity, review/status, and resume evidence. [W2][H2][H3]
5. prompt_hub/rusty-idd remain **upstream** of all implementation loops. [P2][R1]
6. meta-ruvector is exposed through chosen trait/API/MCP/WASM seams **after inventory gating** — never bulk-wired by name. [M2][V2]

### Missing buildable seams (6)

- LifeOS **component registry/status page** backed by envctl `auto-detect/install/verify` JSON. [E1][L2]
- LifeOS **`/ai` route** embedding a local-only Odysseus while preserving LifeOS shell/state. [L1][O3]
- **prompt_hub → rusty-idd → plan-loop → feature-forge** handoff envelope with source citations + test traceability. [P2][R1]
- **weave Damian/job-lane** dispatch for background scans (vs token-heavy broad MCP discovery). [W1][W2]
- **handoff ledger/event projection** into LifeOS Notifications/To-Do/Knowledge. [H2][L2]
- **meta-ruvector memory/vector API selection** for LifeOS knowledge — not a 314-crate firehose. [V1][V2]

---

## Odysseus — QUALIFY / sandbox-adopt (do NOT make canonical yet)

- **License risk:** AGPL-3.0-or-later → **avoid code merge**; integrate only across an API
  boundary (reverse proxy / iframe / API bridge). [O5]
- **Security risk:** requires auth; privileges shell/Python/file/email/MCP/task/skill/memory
  services; raw ChromaDB/SearXNG/ntfy/Ollama/vLLM must stay **internal-only**. [O4]
- **Strict-upgrade path:** envctl component with a **pinned ref** (no floating `latest`),
  local-only bind, managed data dir, health checks, raw-port verification, backup/restore +
  rollback test, and a LifeOS `/ai` adapter route. [E1][E3][O3]

---

## Automation roadmap (Feature-Forge-ready chunks)

**P0**
- Odysseus sandbox component via envctl (pinned install, local bind, health check, auth/secrets, raw-port verify, backup/restore, rollback). [E1][E3][O3][O4]
- LifeOS `/ai` adapter panel (launch / status-check / embed Odysseus; preserve navigation + UI contracts). [L1][L2][O1]
- prompt_hub → rusty-idd plan-loop envelope (planning outputs create/select OpenSpec changes with target DAG, citations, tests, resume packet). [P2][R1][H2]

**P1**
- weave Damian/job-lane background scan runner (default 5-lane planning transport). [W1][W2]
- handoff status projection into LifeOS (ledger state, next safe task, tests, rollback, risks). [H1][H2][L2]
- meta-ruvector memory/vector seam selection (RVF + index traits + mcp-brain/gate/rvAgent). [M2][V1][V2]
- network workspace adapter (network-control off-host fabric, lane local routing, weave-governed obscura). [N1][N2][N3][W2]

**P2**
- autoresearch freshness loop (track Odysseus/ChromaDB/SearXNG/ntfy/model-provider + license/security changes; feed back to prompt_hub/rusty-idd). [O1][O2][O3][O4]

---

## Risk & policy

- **Strict-upgrade only** — no downgrade / destructive reset / legacy removal until the
  replacement is installed, configured, parity-proven, rollback-safe.
- **Odysseus isolation** — AGPL + privileged → isolate by process/API/WebView/reverse proxy
  until license + security gates pass.
- **Raw-service containment** — no ChromaDB/SearXNG/ntfy/Ollama/vLLM/model ports exposed to LifeOS.
- **Production read-only** in the planning phase; permitted writes are planning artifacts +
  additive gate scripts.
- **Source citation required** — every roadmap item carries citations; unverified claims stay out.

---

## How this plan is consumed

- **prompt_hub** seeds the new `prompts/meta-architecture-integration-loop.prompt.yml` from this
  doc: a full-meta-repos plan-loop that maps current architecture and designs cross-repo
  integration, extending (not redoing) the rusty-idd baseline.
- **rusty-idd** archives its active change and uses this as the work-definition baseline for the
  plan-loop → feature-forge envelope (P0).
- **Feature Forge** consumes the P0–P2 roadmap as build chunks, gated through envctl install/verify.

## Source keys

Resolve against the rusty-idd reports under
`/home/drdave/Desktop/meta/rusty-idd/.handoff/loop/plan/` (`reports/`, `findings/`, `graph/`):

- [M1] `meta/.meta.yaml:5-212,276-430` · [M2] `meta/.meta.yaml:292-306`
- [L1] `lifeos/README.md:1-8,81-129` · [L2] `lifeos/AGENTS.md:41-73,157-174` · [L3] lifeos code-graph note (`.git/gitkb/code.db` present, 0 symbols indexed)
- [P1] `prompt_hub/README.md:9-18,39-75,93-175` · [P2] `prompt_hub/prompts/README.md:95-119`
- [R1] rusty-idd `README.md:12-31,42-53` + `docs/rusty-idd/proposal.md:8-15,36-42`
- [W1] `weave/README.md:1-18,25-42,93-176` · [W2] `weave/ARCHITECTURE.md:1-15,25-59,60-100,104-160`
- [H1] `handoff/NORTH-STAR.md:15-58,61-84,87-123,159-169` · [H2] `handoff/docs/ARCHITECTURE.md:1-14,53-63,80-129,148-179` · [H3] `handoff/docs/adr-0018-full-auto-agentic-operation.md:8-19,35-71,73-152`
- [E1] `envctl/README.md:1-14,15-28,46-68,108-123` · [E2] `envctl/docs/ARCHITECTURE.md:9-17,20-44,47-88,92-180` · [E3] `envctl/docs/ADD-REPO.md:1-33,35-85,109-115`
- [N1] `network-control/README.md:1-31,73-98,108-144` · [N2] `lane/README.md:20-28,47-63,113-128,164-175` · [N3] `obscura/README.md:14-35,93-150`
- [V1] `RUVECTOR-CRATE-LEDGER.md:1-5,37-63,120-130,144-155` · [V2] `RUVECTOR-RUNBOOK.md:7-23,33-39,40-115,116-126`
- [O1–O5] `github.com/pewdiepie-archdaemon/odysseus@dev` README / requirements.txt / docker-compose.yml / SECURITY.md / LICENSE (retrieved 2026-06-26)
