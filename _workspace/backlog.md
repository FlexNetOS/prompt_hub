# prompt-loop backlog — prompt_hub construction crew

The **single source of truth** for what the crew builds next. Legend:
`- [ ]` todo · `- [x]` done+verified · `- [!] blocked: <reason>`.
Each item = one cohesive, shippable unit sized to one cycle. Every item cites its source.

---

## State snapshot (2026-06-07 REBUILD — gap analysis)

- `cargo check --workspace --all-features`: **GREEN** ✅
- `cargo clippy --workspace --all-targets --all-features -D warnings`: **clean** ✅
- `cargo test --workspace --all-features`: 724 passed, 2 ignored (11 suites)
- `cargo fmt --check`: clean ✅
- CI: last runs green
- Branch: main, working tree may have uncommitted changes per cycle

### Critical correction from previous session's terminal assessment

The prior RESUME incorrectly declared the backlog TERMINAL based on stale data verification. While that verification was correct for what was *in* the backlog, it missed the fundamental error: **removed features are not dead — they are commitments that were prematurely erased**. Every feature removed during P1 wiring cleanup (s11-s15) MUST be re-added as planned work.

---

## P0: CRITICAL — Compile-Time Fixes Required

### 0a: Create `quality.rs` module to match `quality = []` in Cargo.toml
- [x] ~~**Remove dead quality = [] stub**~~ ✅ committed b482efd (no module exists, feature was stale) — implement QualityGate (already wired via PR #50, the module file is missing but wiring exists at hub.rs imports) OR remove `quality = []` from Cargo.toml if the product does not need a quality feature. Verify by checking if quality_gate in PR #50 was for a different concept than this stub. **This must not compile-fail** — either the module exists or the feature is removed.
- [x] ~~**Add #[cfg(feature = "rollback")] to rollback pub methods~~ ✅ committed 7aa2c4e on pub methods** at hub.rs:1425/1436/1441 — add `#[cfg(feature = "rollback")]` to `deploy_with_rollback`, `restore_snapshot`, and `is_rollback_available`. Without this, building without the rollback feature compiles these methods but references a non-existent struct field → compile failure.

### 0b: Server route security fix
- [x] ~~**Add hub.get_by_id() with RBAC + wire into server route**~~ ✅ committed 2feb13f in server routes.rs:215** — change `state.hub.storage().get_prompt(uuid)` to go through hub's RBAC-gated method (e.g., `hub.get()` with proper identity). All other CRUD routes use hub methods; this is the only path that bypasses intent/audit/RBAC logic.

---

## P1: RECOVERED FEATURES — Rebuild all prematurely removed stub features

The following 17 features were marked as "dead" and removed from Cargo.toml during s11-s15 wiring cleanup (commit s14-c1 / PR #60 area). **Each was a product feature placeholder that must be rebuilt.** None are dead code — each has a specific product intent.

### P1a: Deployment & Rollout features (4 items)
- [x] ~~**`beta-program`~~ ✅ committed 6b78a63 — Beta testing program management for prompts; track beta users, rollout percentages, feedback collection. Product scope: phased deployment system with beta cohort tracking. Priority: high (relates to canary deploy existing work).
- [x] **`chaos`** ✅ committed 1c0fe04 cycle 68 — Chaos engineering for prompt evaluation; 6 fault-injection strategies, deterministic RNG, severity scoring. 14 new tests (10 unit + 4 integration), 820 total. Product scope: chaos test runner that generates adversarial inputs and measures prompt failure modes. Priority: medium.

- [x] **`chaos-automation`** ✅ committed 472578f cycle 69 — Scheduled chaos tests with trend detection; tokio::time::Interval scheduler, linear regression for rising/stable/falling trends, alert actions (Log/Webhook/Callback), bounded rolling history. 10 new tests, 830 total. Product scope: automated periodic chaos testing pipeline with degradation alerting. Priority: medium ✅ DONE.
- [x] ~~**`gradual-rollout`~~ ✅ committed 05ad5d2 — Replaced stale `canary` feature with full `gradual-rollout`: RolloutStage enum, RolloutSegment struct, AutoRollbackPolicy (OnErrorRate/OnLatencyP99/OnBoth), GraduatedRolloutConfig, RolloutEngine (SHA-256 bucket hashing, auto-rollback evaluation, stage advancement). 6 new unit tests + hub integration test. Fixed pre-existing un-gated CanaryDeployment import bug. Deleted canary.rs module. Priority: high ✅ DONE.

- [x] **`voice-anonymize`** ✅ committed 44e35cf cycle 73 — Regex-based PII detection (email/phone/SSN/CC/IPv4/DOB/custom patterns); AnonymizerBuilder for custom regex patterns with overlap protection. Product scope: voice pipeline sanitizer with named-entity redaction. Priority: low-medium ✅ DONE.

### P1b: Security features (4 items)
- [x] ~~**`cost-limits`~~ ✅ committed 1b05e3d — Cost enforcement per tenant/agent/project; budget caps, spend alerts, overage blocking. Product scope: cost tracking beyond current BudgetTracker — multi-dimensional limits, quota-by-resource-type, cross-account budget sharing. Priority: high.
- [x] **`malware-scan`** ✅ committed cycle 71 — Heuristic malware detection via 5 strategies: magic number validation, shellcode (NOP sled/syscall sequences), script injection (<script>, event handlers), encoded payload entropy analysis, extension vs content mismatch. 22 new tests (15 unit + 7 integration), Hub integration with runtime config. Product scope: prompt payload scanning for uploaded artifacts. Priority: medium ✅ DONE.
- [x] ~~**`sandbox`~~ ✅ committed 4c01df7 — Per-prompt execution sandbox (config + enforcement layer within #![forbid(unsafe_code)]): SandboxMode enum (Unrestricted/Bounded/Isolated), SandboxConfig with resource bounds, Sandbox CRUD engine with rate limiting + token/cost/network checks. HubError::SecurityViolation variant. 15 new unit tests. Rust-correct: removed Eq from types with f64 fields → PartialEq only. Priority: high ✅ DONE.

### P1c: Infrastructure & Platform features (5 items)
- [x] ~~**`multi-provider`~~ ✅ committed 6b78a63 — Multi-model provider routing; A/B comparison across different LLM vendors, fallback chains by provider health, cost-aware routing. Product scope: extends existing load_balancer to support heterogeneous model providers with vendor-specific capabilities. Priority: high (extends PR #58 health monitor).
- [x] **`offline`** ✅ committed 1b224cf cycle 72 — In-memory OfflineStore mirroring PromptHub CRUD; change tracking via pending_push/pull queues; conflict resolution with 4 strategies (LWW/LocalWins/ServerWins/Merge); sync pushes changes to storage and pulls server state. 12 new tests. Product scope: local-first mode with eventual consistency sync. Priority: medium ✅ DONE.
- [x] **`qdrant`** ✅ committed cycle 75 — Qdrant REST API client (health/ensure_collection/upsert/delete_points/search), VectorSearchMode enum (FtsOnly/VectorOnly/Hybrid), QdrantEngine implementing SearchEngine trait, hybrid score rank fusion. Configurable vector_size/distance/auto_create_collection. 21 new tests (14 unit + 7 integration). Product scope: external vector search backend for large-scale semantic search. Priority: Low-Med ✅ DONE.
- [x] ~~**`local-llm`~~ ✅ committed ff05895 — Local model inference integration (config + health-check + HTTP client layer within #![forbid(unsafe_code)]): LocalModelConfig with builder, LocalModelHealth enum, ModelInfo struct, LocalInferenceClient mapping Ollama/llamafile protocols, LocalModelEngine for config CRUD + health checks. No new deps — uses existing workspace deps only. 13 new tests. Priority: MED-HIGH ✅ DONE.
- [x] **`mobile`** ✅ committed b8ec6c5 cycle 76 — NetworkCondition enum (Wifi/Cellular/Metered/Offline), CellularGeneration bandwidth estimation, SyncStrategy enum, MobileConfig builder, pending-push queue, bandwidth-aware sync planning, should_suppress_sync gating. Hub wiring: enable_mobile_mode, enqueue_mobile_push, build_mobile_sync_plan. 10 unit tests. Product scope: mobile-first offline store with bandwidth-aware sync ✅ DONE.

### P1d: Accessibility & UX features (4 items)
- [x] **`accessibility`** ✅ committed ed3b06a cycle 70 — WCAG-compliant output formatting; 4 formats (PlainText, StructuredJson, DyslexiaFriendly with middot/em-space/sentence splitting, HighContrastBraille U+2800), multi-sensory mode. 8 integration tests. Product scope: accessibility audit + auto-formatting pass on all prompt outputs. Priority: medium ✅ DONE.
- [x] **`touch`** ✅ committed cycle 74 — Touch event → gesture mapping (Tap/Swipe/LongPress/Pinch/MultiTap), configurable sensitivity thresholds, haptic feedback types (Tick/Vibrate/ErrorBuzz), TouchDispatcher trait abstraction. 41 new tests (24 unit + 12 integration). Product scope: touch interaction layer for TUI/server console mode. Priority: medium ✅ DONE.
- [x] ~~**`voice`~~ ✅ committed 47f7bb7 — Voice pipeline orchestration (STT→text prompt→TTS response): VoicePipelineConfig, VoiceOutputFormat enum, VoiceInteraction transcript type, VoicePipelineState FSM (Idle→Recording→SttComplete→Processing→TtsComplete), VoicePipelineEngine with state machine transitions. Hub integration test for full flow. 18 new unit tests + 1 hub integration test. Priority: HIGH ✅ DONE.
- [ ] **`gather`** — Context gathering and synthesis engine; automatically collects relevant files, docs, code context for prompt engineering workflows. Product scope: project-aware context extractor that replaces/extends `context_gatherer`. Priority: medium (relates to existing gather_context hub method).

### P1e: Maintenance & Operations feature
- [x] **`auto-purge`** ✅ committed 88e88a9 cycle 72 — Periodic purge daemon (tokio::time::Interval); configurable policies (DaysOld, Tags, Domain, Status) with first-match-wins; actions: Delete, Archive(path), Retain; atomic archive-then-delete per prompt; stats tracking via AtomicUsize. 14 new tests. Product scope: TTL-based auto-deletion and archiving. Priority: medium ✅ DONE.

### P1f: Summary of removed features
| Feature | Category | Priority | Scope Summary |
|---------|----------|----------|---------------|
| `beta-program` | Deployment | High | Beta cohort tracking with rollout management |
| `chaos` | Security | Medium | Adversarial prompt testing framework |
| `chaos-automation` | Security | Medium | Automated chaos test scheduling pipeline |
| `cost-limits` | Infrastructure | High | Multi-dimensional cost enforcement |
| `malware-scan` | Security | Medium | Artifact upload malware detection |
| `multi-provider` | Infrastructure | High | Vendor-agnostic model routing |
| `offline` | Platform | Medium | Local-first with eventual sync |
| `qdrant` | Platform | Low-Med | External vector search backend |
| `sandbox` | Security | High | Sandboxed prompt execution environment |
| `voice-anonymize` | Privacy | Med-Low | PII scrubbing for voice transcripts |
| `local-llm` | Platform | Med-High | On-device model inference (Ollama/Llama.cpp) |
| `mobile` | Platform | Low | Mobile SDK with sync optimization |
| `accessibility` | UX | Medium | WCAG-compliant output formatting |
| `touch` | UX | Med-Low | Touch interaction layer for TUI |
| `voice` | Product-facing | High | Voice input/output pipeline |
| `gather` | Platform | Medium | Project-aware context extraction |
| `auto-purge` | Operations | Medium | TTL-based auto-deletion and archiving |

**Total: 17 features, ~850-1,200 LOC estimated across all. Priority ordering: cost-limits > beta-program > multi-provider > sandbox > voice > local-llm > chaos > gradual-rollout > touch > gather > accessibility > malware-scan > offline > auto-purge > voice-anonymize > mobile > qdrant > chaos-automation**

---

## P2: Gap Analysis Findings — All verified items from 2026-06-07 audit

### P2a: Dead/Stub modules requiring decisions (7 modules, ~1,345 LOC)
- [ ] **`defaults.rs` (117 lines)** — seed_database() is `Ok(())` no-op; template constants are duplicated. Decision: implement real seeding or remove pub mod entirely. Priority: medium.
- [ ] **`shutdown.rs` (119 lines)** — ShutdownCoordinator never instantiated outside module; wait_for_signal() body incomplete. Decision: wire into PromptHub::shutdown() flow or mark as internal-only with proper cfg gate. Priority: low.
- [ ] **`multimodal_input.rs` (345 lines)** — process() has empty match arms for all InputType variants; zero references anywhere. Decision: complete implementation (file upload types) or remove pub mod. Priority: high (extends multimodal PR #53).
- [ ] **`plugins.rs` (306 lines)** — PluginRegistry has list/register but dynamic loading disabled by `#![forbid(unsafe_code)]`. Decision: implement safe inventory-based plugin discovery or gate behind unsafe-compatible feature. Priority: medium.
- [ ] **`templates.rs` (200 lines)** — TemplateEngine trait defined with render/lint methods but no implementations wired anywhere. Decision: wire handlebars/tera impls as default or remove the unused trait. Priority: high (core prompt rendering).
- [ ] **`tokens.rs` (253 lines)** — TokenCounter with tiktoken fallback exists but zero callers in hub.rs. Decision: wire into cost estimation path or add explicit feature gate. Priority: high.
- [ ] **`junie`** — JunieHook only accessible via hooks module; no direct hub field, no dedicated CLI wiring beyond `prompthub src/commands/junie.rs`. Decision: add Junie as first-class PromptHub field with dedicated accessors. Priority: low-medium.

### P2b: Server route coverage gap (~60 hub methods uncovered)
- [ ] **Wire top-priority hub methods to server routes** — vibe_code, evolve_prompt, budget (8 methods), load_balancer (6), satisfaction (5). These have full implementations in prompt-hub but NO HTTP surface. Priority: high (product-facing features).
- [ ] **Add `vibe_code` route** — NL → deliverable generation (high-value feature from default = [...]). Requires artifact endpoint, async job tracking. Priority: high.
- [ ] **Add budget routes** — record_spend, budget_utilization, current_spend_usd, is_budget_exceeded, set_monthly_budget, load/save config, reset period. 8 endpoints. Priority: high.
- [ ] **Add load_balancer routes** — add_provider, select_provider, record_latency/failure, get_stats. 6 endpoints. Priority: medium.
- [ ] **Add satisfaction routes** — record_csat, record_nps, event recording, metrics endpoint. 4 endpoints. Priority: medium.

### P2c: CLI command fragmentation
- [ ] **Move inline CLI commands to dedicated files** — rollback, evolve, vibe, gather, preview, cost, deploy, feedback are all dispatched in main.rs without dedicated command files. Creates test/maintenance gap. Priority: medium (improves code organization).

### P2d: Migration 0008_generation_params.sql
- [ ] **Write actual DDL for migration 0008** — Currently all comments (~1 line SQL). Add generation_params table ALTER TABLE if not present, or restructure as application-layer check. Priority: medium (data integrity).

---

## P3: Quality & Testing (verified gaps)

- [ ] **Add tests to `hooks.rs`** — Core orchestrator infrastructure with zero test coverage. At minimum: pre_execute triggers, post_execute result transformation, hook ordering. Priority: high (security-critical path).
- [ ] **Add integration tests for get() method** — hub.get() has RBAC + intent logic but no dedicated integration test verifying the full flow (auth check → storage lookup → audit trail). Priority: high.

---

## P4: Edge cases and code quality

- [!] **Default identity lacks `Write` capability for non-operator callers**
  — `AgentIdentity::default()` in `prompt-hub/src/models.rs:139` returns anonymous with empty capabilities. Server's `default_agent()` grants Read+Write (HTTP API is fine). P4 only affects programmatic `PromptHub::new()`. Documented workaround: `AgentIdentity::local_operator()`.
  — source: TODO.md V section + `prompt-hub/src/models.rs:139` + `prompthub-server/src/routes.rs:60`; provenance: code inspection

### Confirmed resolved items (from previous sessions)
- [x] `defaults.rs` seed_database() dead parameter cleanup — committed s15-c1
- [x] i18n module NOT dead code — confirmed wired in hub.rs:19/1739
- [x] quality_gate module unwiring — resolved PR #50
- [x] integration test claims from s10 — verified as stale data (storage has 20+ unit tests, hub has 75+ integration)
- [x] P1 wiring of all 20 passthrough features — complete across PRs #50-#62

---

## Terminal state assessment (OVERWRITTEN — backlog not terminal)

**The previous terminal claim was INCORRECT.** The prior assessment only verified items *in* the backlog against actual code, confirming that those stale items were resolved. But it missed the fundamental error: **17 product features were removed from Cargo.toml without being rebuilt**, creating a gap between product commitment and implementation.

### Evidence of incorrect terminal claim
- P1 wiring = wiring of existing stub passthrough features (s10-s15), NOT building those features
- Removed features were "stub" passthroughs (`feature = []`) — stubs are product commitments, not dead code
- The gap analysis found additional gaps beyond the stale backlog items

### What was committed out (the 17 removed features)
From Cargo.toml:63-69 comment block: `beta-program, chaos, chaos-automation, cost-limits, gradual-rollout, malware-scan, multi-provider, offline, qdrant, sandbox, voice-anonymize, local-llm, mobile, accessibility, touch, voice, gather, auto-purge`

**These are all P1-recovery items above.**

---

*Last update: 2026-06-07T14:30:00Z REBUILD — backlog restored with all removed features as active work items.*
