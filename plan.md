# Plan: prompt_hub Rust Workspace Build

## Overview
Build a complete `prompt_hub` Rust workspace following BUILD_PROMPT_HUB_LIBRARY_v4.6.md exactly.
3 crates: `prompt-hub` (lib), `prompthub` (CLI bin), `prompthub-server` (HTTP bin).
Rust 2024 Edition, MSRV 1.91.1, `#![forbid(unsafe_code)]`, libsql (NOT sqlite).

## Skill: vibecoding-general-swarm (Mode A - multi-agent, git worktrees)

## Stages

### Stage 0 - Orchestrator Setup (Sequential)
- Write SPEC.md from the full v4.6 specification
- Initialize git repo at /mnt/agents/output/project/
- Create workspace skeleton: Cargo.toml, rust-toolchain.toml, directory structure
- Create branches for parallel work

### Stage 1 - Parallel Subagent Work (Group by dependency)
**Group 1A: Core Library Foundation** (Alpha agent)
- prompt-hub/src/models.rs - All structs, enums, types
- prompt-hub/src/error.rs - thiserror hierarchy
- prompt-hub/src/lib.rs - Module declarations, re-exports
- prompt-hub/Cargo.toml - Full dependencies with features

**Group 1B: Storage + Config + Defaults + Templates** (Beta agent)
- prompt-hub/src/storage.rs - libsql database layer, transactions
- prompt-hub/src/config.rs - XDG config loading + hot-reload
- prompt-hub/src/templates.rs - Handlebars/Tera template engines
- prompt-hub/src/defaults.rs - Seed data + base templates
- migrations/ - SQL migration files

**Group 1C: Auth + Audit + Lock + Sanitize** (Gamma agent)
- prompt-hub/src/auth.rs - RBAC + AgentIdentity + ownership transfer
- prompt-hub/src/audit.rs - Audit logging + tamper evidence
- prompt-hub/src/lock.rs - LockManager with TTL and heartbeat
- prompt-hub/src/sanitize.rs - Prompt injection detection + plugin trait

**Group 1D: Search + Swarm + Sync** (Delta agent)
- prompt-hub/src/search.rs - FAST/SMART/Hybrid search engines
- prompt-hub/src/swarm.rs - Bundle + handoff + consistency
- prompt-hub/src/sync.rs - WebSocket + file watcher + split-brain

**Group 1E: Automation Engine Modules** (Epsilon agent)
- prompt-hub/src/vibe.rs - Vibe Coding engine
- prompt-hub/src/context_gatherer.rs - Auto-context-gathering
- prompt-hub/src/fallback.rs - Auto-fallback chain
- prompt-hub/src/preview.rs - Auto-preview generation
- prompt-hub/src/summarizer.rs - Plain-English result summarization
- prompt-hub/src/confidence.rs - Confidence scoring

**Group 1F: Tier 4+5 Advanced Modules** (Zeta agent)
- prompt-hub/src/evolution.rs - Genetic algorithm
- prompt-hub/src/pollination.rs - Cross-agent pattern sharing
- prompt-hub/src/tokens.rs - Token counting + cost estimation
- prompt-hub/src/i18n.rs - Internationalization
- prompt-hub/src/multimodal.rs - Multi-modal prompt support
- prompt-hub/src/privacy.rs - Privacy scan for secrets/PII
- prompt-hub/src/quality_gate.rs - Quality gate
- prompt-hub/src/rollback.rs - Safe deployment with auto-rollback
- prompt-hub/src/multimodal_input.rs - Voice/screenshot/file processing
- prompt-hub/src/learn.rs - Auto-learning from feedback
- prompt-hub/src/cost.rs - Cost estimation

**Group 1G: Core Hub + Server** (Eta agent)
- prompt-hub/src/hub.rs - Core PromptHub engine (depends on all other modules)
- prompt-hub/src/metrics.rs - OpenTelemetry + custom metrics
- prompt-hub/src/health.rs - Health check aggregation
- prompt-hub/src/shutdown.rs - Graceful shutdown coordinator
- prompt-hub/src/plugins.rs - Plugin system
- prompthub-server/src/main.rs, server.rs, routes.rs, middleware.rs, openapi.rs

**Group 1H: CLI + Tests + DevOps** (Theta agent)
- prompthub/src/main.rs, cli.rs, tui.rs
- prompthub/src/commands/ handlers
- tests/ - Integration tests
- benches/ - Criterion benchmarks
- examples/ - Working examples
- docker/ - Dockerfile, docker-compose.yml
- docs/ - Architecture, ADRs, runbooks
- README.md, CI/CD config

### Stage 2 - Merge Integration
- Merge all branches into main
- Resolve conflicts
- Run cargo check / cargo test
- Fix integration issues

### Stage 3 - Validation & Delivery
- Verify compilation with all feature combinations
- Run tests
- Verify spec compliance
