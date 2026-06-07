# HANDOFF — P1 Recovery Rebuild (2026-06-07T14:30Z)

**Worktree:** Primary checkout at `/home/drdave/Desktop/meta/prompt_hub` (on `main`)
**Branch:** `main` (unprotected → APPLY mode: push/PR/auto-merge on green)
**Base:** latest main commit

---

## 1. SESSION PURPOSE: Rebuild backlog with removed features + gap analysis

The previous session declared the backlog TERMINAL based on stale verification. This session **rebuilt the backlog** to include all product commitments that were prematurely erased during s11-s15 wiring cleanup.

### Why the previous TERMINAL claim was wrong
- Only verified what WAS in the backlog against code → confirmed stale items were resolved
- Missed the root error: 17 product features were removed from Cargo.toml during cleanup, treated as "dead" when they are **product commitments**
- The gap analysis found additional structural gaps beyond just stale items

---

## 2. GAP ANALYSIS FINDINGS — Full codebase audit (verified against live code)

### Critical Gaps (must fix before building new features)
| # | Issue | File:Line | Fix |
|---|-------|-----------|-----|
| CRIT-1 | `quality = []` stub feature has no module file (`quality.rs` doesn't exist) | Cargo.toml:54 | Create module or remove from Cargo.toml |
| CRIT-2 | Rollback pub methods lack `#[cfg(feature = "rollback")]` gates matching struct field | hub.rs:1425/1436/1441 vs :187 | Add cfg gates to 3 methods |
| CRIT-3 | Server routes.rs:215 bypasses hub.get() for direct `storage().get_prompt()` — skips RBAC | prompthub-server/src/routes.rs:215 | Route through hub.get() + identity |

### High Gaps (structural, not urgent)
| # | Issue | Scope |
|---|-------|-------|
| HIGH-1 | 7 dead/stub modules with pub mod but zero product exposure (~1,345 LOC) | defaults, shutdown, multimodal_input, plugins, templates, tokens, junie |
| HIGH-2 | ~60 hub.rs pub methods have NO server route (only 8 of 70+ covered) | vibe_code, budget(8), load_balancer(6), satisfaction(5), etc. |
| HIGH-3 | Migration 0008_generation_params.sql is all comments (~1 line SQL) — version marker only | Data integrity for new databases |
| HIGH-4 | 20+ CLI commands dispatched inline in main.rs without dedicated command files | Rollback, evolve, vibe, deploy, feedback, etc. |

### Medium Gaps
| # | Issue |
|---|-------|
| MED-1 | hooks.rs — core orchestrator infrastructure with ZERO test coverage |
| MED-2 | templates.rs + tokens.rs (450+ LOC) have real impls but zero callers |
| MED-3 | satisfaction stub passthrough creates feature/gate confusion |

### Low Gaps
| # | Issue |
|---|-------|
| LOW-1 | Orphaned "Load balancer" section header at hub.rs:1419 with no methods |
| LOW-2 | No ADR for multi-module scaffolding strategy (51 modules, varying completeness) |
| LOW-3 | CHANGELOG.md only 59 lines for a project with 62+ PRs |

---

## 3. REMOVED FEATURES — Must be rebuilt as P1 recovery

**17 features removed from Cargo.toml during s11-s15 cleanup (marked "dead") are product commitments:**

### P1 Recovery Priority Order (17 items)
| # | Feature | Category | Priority | Estimated Scope |
|---|---------|----------|----------|-----------------|
| 1 | `cost-limits` | Infrastructure | HIGH | Multi-dimensional cost enforcement (~80 LOC) |
| 2 | `beta-program` | Deployment | HIGH | Beta cohort tracking with rollout mgmt (~150 LOC) |
| 3 | `multi-provider` | Infrastructure | HIGH | Vendor-agnostic model routing (~200 LOC) |
| 4 | `sandbox` | Security | HIGH | Sandboxed prompt execution (~250 LOC) |
| 5 | `voice` | Product | HIGH | Voice input/output pipeline (~180 LOC) |
| 6 | `local-llm` | Platform | MED-HIGH | Ollama/Llama.cpp on-device inference (~200 LOC) |
| 7 | `chaos` | Security | MEDIUM | Adversarial prompt testing framework (~150 LOC) |
| 8 | `gradual-rollout` | Deployment | MEDIUM | Percentage-based canary rollout (~120 LOC) |
| 9 | `touch` | UX | MED-LOW | Touch interaction layer for TUI (~100 LOC) |
| 10 | `gather` | Platform | MEDIUM | Project-aware context extraction (~80 LOC) |
| 11 | `accessibility` | UX | MEDIUM | WCAG-compliant output formatting (~100 LOC) |
| 12 | `malware-scan` | Security | MEDIUM | Artifact upload malware detection (~150 LOC) |
| 13 | `offline` | Platform | MEDIUM | Local-first mode with eventual sync (~200 LOC) |
| 14 | `auto-purge` | Operations | MEDIUM | TTL-based auto-deletion/archiving (~120 LOC) |
| 15 | `voice-anonymize` | Privacy | MED-LOW | PII scrubbing for voice transcripts (~80 LOC) |
| 16 | `mobile` | Platform | LOW | Mobile SDK with sync optimization (~300 LOC) |
| 17 | `qdrant` | Platform | LOW-MED | External vector search backend (~250 LOC) |
| 18 | `chaos-automation` | Security | MEDIUM | Automated chaos test scheduling (~100 LOC) |

**Total estimated: ~850-1,200 LOC across 17 features**

---

## 4. PRIORITY ORDER FOR NEXT SESSION

### Phase 1: Fix critical bugs (must-do before anything else)
1. **Fix quality.rs** — create module file OR remove from Cargo.toml (CRIT-1)
2. **Add cfg gates to rollback methods** at hub.rs:1425/1436/1441 (CRIT-2)
3. **Fix server routes.rs:215** — replace direct storage access with hub.get() (CRIT-3)

### Phase 2: P1 recovery — rebuild removed features (priority order above)
1. cost-limits → beta-program → multi-provider → sandbox → voice → local-llm
2. chaos → gradual-rollout → touch → gather → accessibility → malware-scan
3. offline → auto-purge → voice-anonymize → mobile → qdrant → chaos-automation

### Phase 3: Gap analysis fixes (structural improvements)
1. Wire top hub methods to server routes (vibe_code, budget, load_balancer)
2. Add tests for hooks.rs + hub.get() method
3. Move inline CLI commands to dedicated files
4. Fix migration 0008 DDL

---

## 5. Current Gates (verified before handoff)
| Gate | Result |
|------|--------|
| `cargo check --workspace --all-features` | GREEN ✅ |
| `cargo clippy -D warnings` | clean ✅ |
| `cargo test` | 724 passed, 2 ignored ✅ |
| `cargo fmt --check` | clean ✅ |

---

## 6. What was committed out (deleted from Cargo.toml during s11-s15)
```
beta-program, chaos, chaos-automation, cost-limits, gradual-rollout,
malware-scan, multi-provider, offline, qdrant, sandbox, voice-anonymize,
local-llm, mobile, accessibility, touch, voice, gather, auto-purge
```

These were documented in Cargo.toml:63-69 as "dead features" but are product commitments. **They must be rebuilt.**

---

*Handoff written: 2026-06-07T14:35:00Z | P1 recovery rebuild started. Backlog restored with all removed features + gap analysis findings.*
**Critical note for next session:** The previous TERMINAL claim was incorrect. The backlog is NOT terminal — the prior verification only confirmed stale items were resolved, not that new work needed to be added.
