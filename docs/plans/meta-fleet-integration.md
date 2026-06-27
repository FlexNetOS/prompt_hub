# Meta Fleet — Architecture & Integration Plan (full-fleet)

> **Status:** synthesized plan (decision-grade, evidence-backed). One `meta-architecture-integration-loop` cycle.
> **Cycle:** `2026-06-27-fleet-cycle1` · run dir `prompt_hub/_workspace/planning-loop/2026-06-27-fleet-cycle1/`.
> **Extends, does not redo:** the rusty-idd baseline [`lifeos-meta-front-door.md`](./lifeos-meta-front-door.md)
> (front-door verdict + 16-component matrix). This doc opens that black box across the **full `.meta.yaml` fleet (~70 members)**.
> **Method:** 5 read-only Opus background agents mapped the fleet in clusters (spine / envctl / agent-substrate /
> front-door+intent+harness / inference+automation+external). Every claim is `file:line`/path-cited and fail-closed
> (a missing file / empty result is a *finding*, not a pass). `git kb code` symbol graph is **not indexed** for these
> repos (`symbols --json` → `{"count":0}`), so structural edges are Cargo path-deps + `run_plugin`/`Command::new`
> call-sites cross-checked against `.meta.yaml depends_on` — confidence HIGH on wiring, MEDIUM on call-graph internals.

---

## Verdict (extends the baseline)

The baseline verdict **HOLDS and is now code-grounded, not advisory**: LifeOS is the owner-facing **UI front door**
(a shell, not a monolith); each subsystem keeps its **authoritative engine**; integration is **additive named seams**,
strict-upgrade-only. Three corrections raise the fidelity:

1. **There are two front doors, and both are now in source** (not just rusty-idd reply #178):
   `rusty-idd/crates/knowledge/src/lib.rs:3617-3636` encodes `capability:user-front-door` (LifeOS) and
   `capability:prompt-front-door` (prompt_hub→handoff/rusty-idd). The **intent** front door is the pipeline
   **harness_hub (interpreter) → prompt_hub (store) → rusty-idd (lifecycle) → harness-agent-rs (executor)**.
2. **LifeOS is understated by the baseline.** At HEAD it is a real multi-crate Rust workspace (`lifeos-core` auth/storage/MCP,
   `lifeos-daemon` Pi bridge; `Cargo.toml:18-26`) shipping a **durable Tauri AI-provider runtime** (OS-keyring keys,
   3 providers; `AGENTS.md:162-174`) and an already-chosen **MCP-REST ruvector mirror** seam
   (`crates/lifeos-core/src/storage/ruvector.rs`). "Skeleton / lightweight chat / `source` unused" is stale.
3. **The harness execution layer is missing from the baseline model** — `harness-agent-rs` (a real Rust DAG runtime,
   ~56% ported, 2193 tests green, bin `har`) is *absent*, yet it is the executor for what `harness_hub` describes.
   This plan adds it as a first-class component and names the missing parser seam between them.

**Strict-upgrade-only governs everything:** no downgrade, destructive reset, or legacy removal until a replacement is
installed, configured, parity-proven, and rollback-safe. Integration boundaries are **additive seams over existing
engines** — never collapse a subsystem's authoritative engine into another repo.

---

## I4 — ASCII architecture diagrams

### Current fleet (real edges today, evidence-cited in the seam catalog)

```
            ┌──────────────────────── meta host  (meta_cli, pkg `meta`) ────────────────────────┐
            │ subprocess plugin protocol = meta_plugin_protocol   [meta_plugin_api = DEAD/orphan] │
            │  meta-git · meta-project · meta-rust                                                 │
            │  meta-dashboard ──shell `envctl dashboard --json`──▶ envctl                          │
            │  meta-env (shipped BY envctl) ──ExecutionPlan `envctl <verb>`──▶ envctl              │
            │  meta-mcp ──29 tools (HYBRID: lib calls + shells `meta`/`git`)──▶ AI clients         │
            └───────┬───────────────────────────────────────────────────────────┬────────────────┘
       path-dep     │ loop_lib (cmd substrate) ◀── meta_cli AND envctl/engine     │ owns env/toolchain/secrets
                    ▼                                                             ▼
   envctl (engine ⊕ cli ⊕ gui) ── manifest/*.toml ──▶ toolchains · yazelix · ohmyzsh · rtk · [vox→/usr/local ⚠]
        │ agent-env (absorbed kasetto v3.2.0) ──sync──▶ .claude/.codex   ◀──source── agent-skills
        │ secretd/secretctl ──mint {token,expires_at_unix} (FROZEN)──▶ flexnetos_github_app
        │                    ──auto-inject bearer (real key never on wire)──▶ child tools
        ▼
   handoff (hf kernel) ◀──lease bridge── weave ;  grit (locks only) ;  icm (memory hooks, C-linked peer)
        ▲ FLEET ledger: ledger.events.jsonl (committed, truth) + ledger.db (gitignored redb cache)  [ADR-0017/0018]
   lane (network-plane spine) ──CA governor──▶ obscura (browser engine) ; ◀──relay git-dep── network-control (fabric)
   weave ──weave_web dispatcher (permission/lease)──▶ obscura

   ── INTENT / FRONT-DOOR ───────────────────────────────────────────────────────────────────────
   prompt_hub (store) ──prompt──▶ rusty-idd (intent lifecycle; vendors imports/handoff ⊕ imports/prompt_hub = union D1)
   rusty-idd ──thin-adapter `rusty-idd deploy` (SessionStart `rusty-idd next`)──▶ lifeos + ALL fleet vendors
   harness_hub (packaged harnesses) ──eject.sh──▶ envctl/.claude (ejected mirror)
   harness-agent-rs (Rust DAG runtime `har`, ~56%) ── UNWIRED: no harness_hub-markdown→WorkflowDefinition parser
   lifeos (REAL app) ──MCP-REST mirror──▶ meta-ruvector / cognitum-seed
   atc · agent · hermes-agent(Python) = orchestration surfaces, UNWIRED to the intent front door

   ── INFERENCE (authority UNRESOLVED) ──  shimmy (OpenAI @127.0.0.1:11435, NO envctl gate)  ‖  ruvllm (crate, 0 consumers)
   ── AUTOMATION ──  n8n ──lane──▶ https://n8n.test ;  n8n-mcp ──▶ agents
   ── CI PLANES ──   flexnetos_github_app (webhook HMAC / App-JWT) ──dispatch(P2)──▶ flexnetos_runner
                     flexnetos_runner ──delegate──▶ loop_lib(build) · atc(agent) · hf(loop-cycle) · weave(lease)
```

### Target integration (front-door end-state; reconciled with the baseline DAG)

```
        UI FRONT DOOR                                         INTENT FRONT DOOR (two-layer, owner D3)
        ─────────────                                         ────────────────────────────────────────
   LifeOS (shell) ◀─status/registry JSON─ envctl     harness_hub ─interpret─▶ prompt_hub ─intent─▶ rusty-idd
       │  ▲  ▲                                         (markdown harnesses)     (store)        (lifecycle ⊕ handoff
  /ai  │  │  └─handoff status projection─ handoff                                                union D1)
  panel▼  └─memory/vector (MCP-mirror seam)─ meta-ruvector                                          │ ready goal/spec
  Odysseus                                                            WorkflowDefinition │ (NEW seam G4)│
 (sandbox/API only — AGPL)                                                               ▼              ▼
       │                                                                       harness-agent-rs ◀─── feature-forge
       │ install/verify authority (ordering rule)                                (DAG runtime)          │ build chunks
       └──────────────────────────────────────────── envctl ◀──────────────────────────────────────────┘
                                                         │ secretd mint ─▶ github_app ─dispatch─▶ runner (CI plane, G5)
                                                         ▼
                       inference authority = [DECIDE G1: shimmy server ‖ ruvllm crate]  (ollama swap-parity gated)
                       network plane = lane ⊕ obscura(egress) ⊕ network-control(fabric) ;  A2A/leases = weave
```

**Ordering rule (unchanged, now fleet-wide):** install+verify authority flows through **envctl** before LifeOS treats a
component as managed; work-definition authority flows through **prompt_hub → rusty-idd** before Feature Forge builds;
continuity is the **handoff⊕rusty-idd union**; the handoff ledger contract is committed `ledger.events.jsonl` + rendered
text, `ledger.db` gitignored (source-verified ×4: `.gitignore`, `ledger/src/lib.rs`, `hf/src/durability.rs`, `handoff-fleet/src/lib.rs`; ADR-0017/0018).

---

## I5 — Integration design

### Component-ownership matrix (fleet extension — adds the layers the baseline collapsed)

The baseline's 16 rows hold (with the LifeOS corrections). This extension decomposes the substrate cells the baseline
marked "keep as substrate; do not duplicate." New/under-modeled components are **bold**.

| Component | Owns | Outputs / APIs | State / authority | Key gap |
|---|---|---|---|---|
| meta_cli (host) | project graph, plugin discovery+dispatch, worktree, `meta exec` | bin `meta`; subprocess plugin protocol | `.meta.yaml`; spawns `meta-*` | broad public surface = high blast radius |
| meta_plugin_protocol | host↔plugin wire contract + `run_plugin()` | `PluginInfo/Request/ExecutionPlan` | the active protocol | healthy |
| **meta_plugin_api** | legacy in-proc `Plugin` FFI trait | `PluginCreate unsafe fn` | **DEAD — 0 consumers** | retire/archive decision (G7) |
| loop_lib | parallel cmd-exec substrate | `build_command/run_commands` | stateless; used by meta_cli **and** envctl/engine | foundation; healthy |
| meta_core / meta_git_lib | `$META_ROOT` dirs+lock+store; git ops | config/store/lock; clone/worktree/snapshot | on-disk meta state | git_lib path-deps host crate (cycle smell) |
| meta_mcp | MCP server exposing meta ops | 29 `meta_*` tools | **hybrid lib+shell** | two code paths can drift (G7) |
| meta_dashboard_cli | `meta dashboard` | shells `envctl dashboard --json` | stateless; own workspace | fail-closed if envctl absent (correct) |
| envctl (engine ⊕ cli ⊕ gui) | meta-local installs, components, locks, FHS/XDG layout, env/secret path authority | `auto-detect/install/auto-fix/reset/add-repo/lock/doctor/agent/dashboard/secret` (+`--json`) | `envctl.lock`; `$META_ROOT`; **install/verify authority** | layout mid-migration (`.toolchains`→`usr/` FHS) |
| **secrets stack** (secretd/secretctl/secrets-engine/proto/store-libsql) | pure-Rust vault; `ProviderMint`; GitHub-App JWT; child auto-inject | gRPC `Vault`(10)/`Relay`(5); `mint-github → {token,expires_at_unix}` (FROZEN); `secretctl run` injects bearer | libSQL **remote** (no C-SQLite); sole keystore | USB-unlock `RealUsbProbe` TODO |
| **agent-env** (absorbed kasetto) | provisions `.claude/.codex` skills+MCP+commands; SHA-256 asset lock | `envctl agent {sync,lock,…}` | `agent-env.lock`; source=agent-skills | kasetto dual-truth; MCP doc 6 vs lock 7 (G7) |
| handoff (hf) | continuity ledger, claims/leases, packets, fleet rollup, p7 | ~45 verbs; `hf fleet render <member>`; `hf-mcp` | committed `ledger.events.jsonl` (truth) + gitignored redb cache | live picker CWD-relative (HFTASK-0054) |
| weave | A2A mesh: messages/asks/jobs/leases/permissions | ~70 CLI cmds; ONE token-light MCP meta-tool (74 ops) + `weave_web` | "the DB is the broker" (no daemon) | repowire absorbed **messaging-only**; JobRunner/cron deferred |
| grit | intra-repo `file::symbol` git-locks, worktree coord | `init/claim/release/queue/…` | `.grit/registry.db` (C-SQLite, peer) | **unfit as union merge engine** (hash computed, never read; plain `git merge`) |
| icm | persistent cross-session memory | 40+ CLI verbs; opt-in `icm serve` MCP | `~/.local/share/icm` (C-linked) | CLI-default vs MCP-opt-in must stay disambiguated |
| lane | network plane: local HTTPS, TLS proxy, tunnels, governed egress, relay | `start/up/domain/cert/relay/net/web/doctor` | daemon; `~/.lane/` | "workflow spine" label is a misnomer (it's a net plane) |
| obscura | ground-up Rust headless browser + egress engine | `serve/fetch/scrape/mcp`; CDP; `browser_*` MCP | no own egress policy — governed by lane CA | C++ (V8) in JS path; rustls in net path |
| network-control (netctl) | off-host fabric (Omada/switch/modem/VLAN/VPN) | `netctl` CLI/GUI (`--json`) | composes **lane only** (NOT ruvector — baseline N1 corrected) | Python legacy retained (no-downgrade) |
| **lifeos** | owner UI shell + `lifeos-core`/`lifeos-daemon` + durable AI runtime | Vue3/Tauri2; `ai_complete`; daemon | real app; OS-keyring keys; chose MCP-mirror ruvector seam | `/ai` Odysseus panel + registry page unbuilt |
| prompt_hub | prompt source-of-truth; durable intent store; planning prompts | `prompthub` CLI/server/lib; `prompts/*.prompt.yml` | Rust 2024 workspace; FTS5+ONNX | prompt↔rusty-idd-change binding design-only; ADR-0007 citation mis-points (G7) |
| rusty-idd | intent/spec/goal lifecycle; OpenSpec engine; intent control plane | bin `rusty-idd` (`Plan/Spec/Next/Deploy/Harness/Render/…`) | vendors `imports/handoff` ⊕ `imports/prompt_hub` (union) | active OpenSpec change archive pending |
| **harness_hub** | packaged/ejectable harness catalog (intent-front-door *interpreter*) | `registry.json` (10) + `harness/skills`(~40)+`agents`(~35) + `eject.sh` | curation repo (md+json) | not a running binary; interpretation is md executed by Claude |
| **harness-agent-rs** | **Rust runtime that EXECUTES harness DAG workflows** (Archon port) | 17-crate ws; bin `har`; `execute_dag_workflow()` | building, 2193 tests; 5 stub crates | **no harness_hub-md→WorkflowDefinition parser; no hf/weave/grit dep yet** (G4) |
| **atc** | worktree-isolated agent orchestrator, 6-signal health | `atc-core`+`atc-cli`; `run/enqueue/daemon/health/tui` | real Rust ws; GitKB task resolution | no weave/handoff code dep — isolated from intent front door |
| agent | Claude-Code discipline CLI (guard/score/codex) | bin `agent` | real single pkg | utility only; not an orchestrator |
| **hermes-agent** | self-improving multi-platform agent + ACP server | Python; `hermes`/`hermes-acp`; ACP 0.9.0 | real **Python** project | zero meta deps; **language drift** (Python) |
| **shimmy** | local LLM inference (pure-Rust Airframe GPU, GGUF) | OpenAI `POST /v1/chat/completions` @127.0.0.1:11435 | single binary; reuses ollama model dir | **no envctl component; ollama-swap directive unenforced** (G1) |
| meta-ruvector (crates-only) | vector/memory/agent/inference substrate (150 crates) | `ruvllm` (cargo add) + ruvector-core/rvf/rvAgent | crates only | **0 meta path-dep consumers** (G2); prompt's named crates mismatch actual |
| **flexnetos_github_app** | GitHub↔local control plane (webhook/JWT/merge-gate) | axum `/webhook`,`/health`; `fxapp sign/verify` | no credential custody (envctl-sealed) | mint/dispatch/merge-gate typed but P1/P2 unwired (G5) |
| **flexnetos_runner** | CI execution plane (self-hosted runner + UDS dispatch) | admission gates → delegate to kernels | `runner-core` pure policy | live routing maturity unverified (G5) |
| **n8n** | workflow automation (AGPL fork); agent web-access seam | REST/webhook :5678 via lane; n8n-mcp | `_workspace/` loop | AGPL → API/sandbox boundary |
| teri | swarm-intelligence prediction (AGPL) | HTTP API + CLI `teri serve` | rusqlite bundled (C) | AGPL → API boundary if consumed |
| Hubs (plugin/mcp/tool/database/network/vault) | curated registries | `registry.json` catalogs | manifest-only | tool_hub thin (1); **vault_hub no registry.json** |
| Empty/placeholder | `flexnetos_wiki/brain`, `my-wiki`, `assets`, `hooks_hub`(0), `flow_hub`(0) | — | **0 content** | declared-but-not-built (G7) |

### Named-seam catalog (producer → consumer · contract · authority · status)

Consolidated from all 5 clusters; **EXISTS** = wired & source-cited, **PARTIAL** = typed/one side built, **MISSING** = design-only.

| # | Producer → Consumer | Contract | Authority | Status |
|---|---|---|---|---|
| S1 | meta_cli → `meta-*` plugins | `--meta-plugin-info/-exec` JSON → `ExecutionPlan` | host drives, plugin plans | **EXISTS** |
| S2 | `meta env` → envctl | meta-env.rs `ExecutionPlan` runs `envctl <verb>` | meta shape, **envctl impl** | **EXISTS** |
| S3 | `meta dashboard` → envctl | shell `envctl dashboard --json` | envctl owns layout | **EXISTS** |
| S4 | envctl agent-env ← agent-skills | `source: ./agent-skills`; `agent-env.lock` | agent-skills = source | **EXISTS** |
| S5 | secretd → flexnetos_github_app | `mint-github → {token,expires_at_unix}` (FROZEN) | secretd = sole keystore | **PARTIAL** (P1 prod path not live) |
| S6 | secretd → child tools | bearer auto-inject (`secretctl run`) | secretd authoritative | **EXISTS** |
| S7 | weave ↔ handoff | lease bridge → hf claims (HFTASK-0048) | weave leases, hf consumes | **PARTIAL** |
| S8 | lane → obscura | ADR-0001 governed-egress CA proxy | lane governor, obscura trusts | **PARTIAL** (live spawn `--features obscura`) |
| S9 | weave → obscura | `weave_web` MCP dispatcher (permission/lease) | weave gate, obscura engine | **PARTIAL** |
| S10 | lane → network-control | `relay` git-dep feature | lane relay spine | **EXISTS** |
| S11 | prompt_hub → rusty-idd | intent/prompt → OpenSpec change | prompt_hub upstream | **PARTIAL** (no programmatic binding) |
| S12 | rusty-idd → lifeos + fleet | thin-adapter + SessionStart `rusty-idd next` | rusty-idd = intent authority | **EXISTS** |
| S13 | harness_hub → envctl/.claude | `eject.sh` | harness_hub = source | **EXISTS** |
| S14 | **harness_hub → harness-agent-rs** | **markdown harness → `WorkflowDefinition`** | hub describes, runtime executes | **MISSING (G4 — the key new seam)** |
| S15 | rusty-idd ⊕ handoff (union D1) | vendored `imports/handoff`; symbol-merge | converging control plane | **PARTIAL** (no symbol-level merge engine) |
| S16 | envctl `*/verify --json` → lifeos | component-registry/status JSON | envctl install authority | **MISSING** (baseline P0) |
| S17 | handoff → lifeos | `hf fleet render` status projection | hf = authority | **MISSING** (baseline P1) |
| S18 | meta-ruvector → lifeos | **MCP-REST mirror** (lifeos already chose) | substrate → LifeOS | **PARTIAL/diverged** (reconcile baseline V2 to this) |
| S19 | shimmy/ruvllm → agent/atc/lifeos `/ai` | OpenAI HTTP ‖ `ruvllm` crate | **UNDECIDED (G1)** | **PARTIAL** (no chosen authority/gate) |
| S20 | flexnetos_github_app → flexnetos_runner | signed JobSpec over UDS → admission gates | app dispatches, runner executes | **PARTIAL** (P2/P3 fail-closed) |
| S21 | n8n → lane / n8n-mcp → agents | `n8n.test` HTTPS; MCP control | n8n behind lane | **EXISTS / PARTIAL** |
| S22 | vox → Claude agents | `vox serve` MCP (14 tools) | vox → agents | **MISSING** (not in meta `.mcp.json`) |

### Front-door pattern

- **UI front door = LifeOS.** Aggregates/launches/controls via **additive adapters** only: an envctl-JSON component
  registry (S16), a handoff status projection (S17), the already-chosen ruvector MCP-mirror (S18), and a sandboxed
  `/ai` panel (Odysseus, AGPL → API/iframe boundary, never source-merged). Raw services (ChromaDB/SearXNG/ntfy/Ollama/
  vLLM/shimmy port) stay internal-only.
- **Intent front door = harness_hub (interpret) → prompt_hub (store) → rusty-idd (lifecycle) → harness-agent-rs (execute).**
  The missing structural piece is **S14**: nothing parses harness_hub's markdown harnesses into the `WorkflowDefinition`
  schema `harness-agent-rs` executes — today they are executed only by Claude reading the markdown. Building S14 turns
  the packaged harnesses into Rust-runtime-executable DAGs, which is what closes the "intent → execution" loop.
- **Authority directions (the law):** install/verify → envctl; work-definition → prompt_hub→rusty-idd; continuity →
  handoff⊕rusty-idd union; A2A/leases → weave; secrets/mint → secretd; network/egress → lane/obscura/network-control;
  inference → **[DECIDE G1]**; memory/vector → meta-ruvector via the chosen seam.
- **External/AGPL containment:** Odysseus, n8n, teri stay behind an API/sandbox boundary; claude-code/codex are pinned
  vendor forks (never source-merge); hermes-agent (Python) and ruflo (JS) stay standalone, not engine-merged.

---

## I6 — Gap → upgrade table

Each row: target surface · evidence · impact · risk tier (**APPLY** = low-risk in-scope branch→PR; **PROPOSE** =
owner/structural/kernel-class) · acceptance criterion (the falsifiable I8 condition).

| # | Gap | Target surface | Evidence | Tier | Acceptance criterion |
|---|---|---|---|---|---|
| G1 | Inference authority unresolved (shimmy vs ruvllm); ollama-replacement directive unenforced | envctl manifest + owner decision | shimmy has no `manifest/*` component (grep empty); `ruvllm` 0 path-dep consumers; owner doctrine "shimmy = official ollama replacement, don't remove ollama until swap proven" | **PROPOSE** | An envctl inference component installs the chosen engine pinned + a swap-parity gate proving it serves the ollama-compat surface before ollama is removed |
| G2 | meta-ruvector declared substrate but **0 consumers**; baseline "select seam by trait/API" superseded by lifeos's MCP-mirror choice | meta-ruvector ↔ lifeos | grep of network-control/lane/envctl Cargo.toml empty; `lifeos/.../storage/ruvector.rs` | **PROPOSE** | Baseline V2 reconciled to the MCP-mirror decision; first real consumer's seam documented and version-pinned |
| G3 | handoff⊕rusty-idd union has **no symbol-level merge engine**; grit unfit (hash computed, never read; plain git merge) | handoff/rusty-idd + grit | `grit/parser/mod.rs:328-420` write-only hash; `git/mod.rs:221-253` plain merge | **PROPOSE** (kernel-class) | A symbol-aware reconciler dedups the ~95%-shared handoff/rusty-idd crates with grit as the *coordination* substrate, not the reconciler |
| G4 | **Harness execution layer not wired**: no harness_hub-markdown → `WorkflowDefinition` parser; harness-agent-rs absent from the model | harness_hub ↔ harness-agent-rs | `har-contract/src/lib.rs:737` consumes pre-parsed workflows; no parser in either repo | **PROPOSE** | A parser converts a packaged harness into a `WorkflowDefinition` that `har` executes end-to-end on one real harness (differential-drive vs the Claude-executed path) |
| G5 | CI control/exec planes typed but unwired (mint prod path P1; runner dispatch P2/P3 fail-closed) | flexnetos_github_app ↔ secretd ↔ runner | `app-core/src/mint.rs` `UnwiredMinter` default; runner README "P0 scaffold" | **APPLY** (mint path; frozen contract exists) / **PROPOSE** (dispatch) | `fxapp` mints a real installation token through `secretctl mint-github` and the merge-gate posts a required status check |
| G6 | LifeOS front-door seams unbuilt (registry page S16; handoff projection S17; `/ai` Odysseus S18) | lifeos | baseline P0/P1; lifeos has the AI runtime + ruvector mirror but not these | **PROPOSE** | LifeOS renders live envctl component status from `auto-detect --json` and a handoff status feed; `/ai` embeds a local-bound Odysseus |
| G7 | Drift/hygiene sweep | multiple | meta_plugin_api dead; kasetto dual-truth; MCP doc 6 vs lock 7; vox→/usr/local (no-system-depth violation, unmanaged); rtk install-source drift; 4 empty repos + vault_hub/hooks_hub/flow_hub; commands→envctl symlink inversion; claude-plugin `Harmony Labs`/`gitkb` provenance; harness↔grit CLI doc drift; meta_mcp hybrid | **APPLY** (per-item, separate PRs) | each item: retire-or-document decision recorded; vox gets an envctl component; MCP doc reconciled to lock; empty repos get a disposition (build or archive) |

**Integration-tooling evaluation (token cost / currency):** `hf` (verbs source-verified, live), `git kb code` (**unindexed for the fleet — index it or stop citing it as available**), `meta` CLI (healthy; `meta_plugin_api` dead weight), weave (single token-light MCP tool — good; runner half deferred), MCP baseline (7 servers in lock incl. n8n-mcp — reconcile the 6-server doc). No skill overload found in this cycle; recommend **indexing the code graph** so future cycles get real call-graph edges instead of dependency-edge fallback.

---

## I7 — Integration roadmap (extends rusty-idd P0–P2; does not duplicate)

**P0 — unblockers (decide + wire the frozen-contract paths)**
- **D-G1** owner decision: inference authority (shimmy server vs ruvllm crate) → then envctl inference component (pinned, ollama swap-parity gate). *[owner wall]*
- **D-G3** owner decision: handoff⊕rusty-idd symbol-merge approach (grit = coordinator only). *[owner wall, kernel-class]*
- **G5 mint path** (APPLY): wire `flexnetos_github_app` to the frozen `secretctl mint-github` prod path — unblocks the CI control plane (the contract already exists).

**P1 — the execution + front-door layer**
- **G4** harness_hub-markdown → `WorkflowDefinition` parser (S14) — turns packaged harnesses into `har`-executable DAGs.
- **G6** LifeOS component-registry page ← envctl `auto-detect --json` (S16); handoff status projection (S17).
- **G2** reconcile meta-ruvector seam to lifeos's MCP-mirror decision; pin the version; update baseline V2.
- **G5 dispatch** (PROPOSE): wire github_app→runner JobSpec dispatch + admission gates.

**P2 — sandbox + hygiene + freshness**
- **G6** LifeOS `/ai` Odysseus sandbox panel (AGPL → API boundary; pinned envctl component; local bind; backup/rollback).
- **G7** drift sweep (separate APPLY PRs): retire meta_plugin_api; pick kasetto canonical home; vox envctl component
  (off /usr/local); reconcile MCP doc; dispose empty repos; fix provenance + harness↔grit CLI docs; de-hybridize meta_mcp.
- weave Damian/job-lane runner (the deferred execution half) — or record it stays deferred with rationale.
- autoresearch freshness loop (Odysseus/model-provider/license deltas → prompt_hub/rusty-idd).

**Witnessed next actions (Feature-Forge-ready, sequenced):** ① owner decisions D-G1, D-G3. ② `feature:` G5 mint prod path
(envctl/flexnetos_github_app, frozen contract). ③ `feature:` G4 parser (harness_hub↔harness-agent-rs). ④ `feature:` G6
LifeOS registry+projection (lifeos, additive). ⑤ `harden:` G7 items (one PR each). ⑥ index the fleet code graph (`git kb code index`).

---

## I8 — TDD traceability (this is an architecture cycle → seams are design-only)

This loop **authors acceptance criteria + RED-verifies buildable seams; it does not implement them.** At the
**fleet-architecture** level every seam below is **design-only this cycle** — authoring RED tests would require mutating
10+ foreign repos in one cycle, which violates the one-coherent-chunk + single-safe-worktree discipline and the
strict-upgrade law. Per the loop's clause ("a design-only seam with no buildable test is flagged as such, not silently
skipped"), each is flagged with its target test home for the per-seam Feature-Forge cycle that builds it.

| Item | Acceptance criterion (→ becomes the RED test) | Target repo · test path | This cycle |
|---|---|---|---|
| G5 mint | `fxapp` mints a real token via `secretctl mint-github`; merge-gate posts required check | flexnetos_github_app `crates/app-core/tests/mint_e2e.rs` (differential-drive vs `UnwiredMinter`) | **design-only — RED deferred** |
| G4 parser | one packaged harness → `WorkflowDefinition` executes on `har` matching the Claude path | harness-agent-rs `har-dag-executor/tests/harness_parse.rs` | **design-only — RED deferred** |
| G6 registry | LifeOS renders live `envctl auto-detect --json` component status | lifeos `crates/lifeos-core/tests/registry.rs` | **design-only — RED deferred** |
| G1 inference | envctl inference component serves the ollama-compat surface before ollama removal | envctl `crates/engine/tests/inference_component.rs` | **design-only — RED deferred (gated on D-G1)** |
| G7 vox | vox installs under `$META_ROOT` not `/usr/local`; managed by a manifest component | envctl `crates/engine/tests/vox_component.rs` | **design-only — RED deferred** |

**Tests authored this cycle: 0 (architecture/mapping cycle).** Honest fail-closed reason recorded above. The next loop
running with a chosen owner decision (D-G1/D-G3) and a single target repo SHOULD author the RED suite for that one seam.

---

## Confidence & remaining walls

- **Confidence:** HIGH on current-fleet wiring (every edge is a cited Cargo path-dep / `run_plugin` / `Command::new`
  call-site, cross-checked against `.meta.yaml`), the handoff ledger contract (4× source-verified), the grit-unfit
  caveat, and the LifeOS corrections. MEDIUM on call-graph internals (code graph unindexed) and on the breadth leaf
  forks (role/build-state corroborated, symbol-level not).
- **Owner walls (NEEDS-HUMAN):** D-G1 inference authority, D-G3 union merge-engine approach, the disposition of the 4
  empty repos and kasetto's canonical home. These are scope/irreversible decisions — surfaced, not performed.
- **Biggest single lever:** **S14 / G4** — wiring harness_hub's markdown harnesses to the `harness-agent-rs` Rust DAG
  runtime is what converts the entire "intent front door" from Claude-executed prose into a runnable execution layer.
