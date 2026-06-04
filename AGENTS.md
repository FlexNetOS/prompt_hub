# Custom Agents for prompt_hub

This file defines specialized agents for parallel multi-agent development workflows using git worktrees.

## Agent Registry

### 🔵 Alpha: Core Library Foundation
**Specialty**: Type definitions, errors, and core exports

**Crates & Files**:
- `prompt-hub/src/models.rs` — All struct/enum definitions
- `prompt-hub/src/error.rs` — HubError enum and error handling
- `prompt-hub/src/lib.rs` — Module declarations and re-exports
- `prompt-hub/Cargo.toml` — Dependencies and features

**Responsibilities**:
- Design type system for all features
- Define error variants with context
- Maintain public API surface
- Coordinate with other agents on type changes

**Git Worktree**: `worktrees/alpha-core-types/`
**Branch**: `wave9/alpha-core-types`

---

### 🟢 Beta: Storage & Configuration
**Specialty**: Database, migrations, config loading, templates

**Crates & Files**:
- `prompt-hub/src/storage.rs` — libsql database layer
- `prompt-hub/src/config.rs` — XDG config loading and hot-reload
- `prompt-hub/src/templates.rs` — Handlebars/Tera rendering
- `prompt-hub/src/defaults.rs` — Seed data and base templates
- `prompt-hub/migrations/` — SQL migration files (embedded into the lib via `include_str!`)

**Responsibilities**:
- Design and implement database schema
- Create SQL migrations (numbered sequentially)
- Load config from environment and files
- Render prompts from templates
- Seed default data

**Git Worktree**: `worktrees/beta-storage-config/`
**Branch**: `wave9/beta-storage-config`

---

### 🟡 Gamma: Security & Audit
**Specialty**: Authentication, authorization, audit logging, lock management, sanitization

**Crates & Files**:
- `prompt-hub/src/auth.rs` — RBAC and AgentIdentity
- `prompt-hub/src/audit.rs` — Audit logging with tamper evidence
- `prompt-hub/src/lock.rs` — LockManager with TTL and heartbeat
- `prompt-hub/src/sanitize.rs` — Prompt injection detection

**Responsibilities**:
- Implement RBAC with capabilities
- Log all mutations with immutable evidence
- Manage distributed locks with automatic expiry
- Detect and prevent prompt injection attacks
- Integrate security checks into storage operations

**Git Worktree**: `worktrees/gamma-security-audit/`
**Branch**: `wave9/gamma-security-audit`

---

### 🔴 Delta: Search & Synchronization
**Specialty**: Full-text search, vector embeddings, swarm coordination, sync protocol

**Crates & Files**:
- `prompt-hub/src/search.rs` — FTS5 and vector search engines
- `prompt-hub/src/swarm.rs` — Bundle, handoff, consistency checks
- `prompt-hub/src/sync.rs` — WebSocket protocol, file watcher, split-brain detection

**Responsibilities**:
- Implement FAST/SMART/Hybrid search engines using FTS5
- Support vector similarity search with embeddings
- Manage swarm bundles and agent handoffs
- Sync prompts across multiple clients
- Detect and resolve split-brain scenarios

**Git Worktree**: `worktrees/delta-search-sync/`
**Branch**: `wave9/delta-search-sync`

---

### 🟣 Epsilon: Automation Engine
**Specialty**: Vibe Coding, auto-context, fallback chains, preview generation, confidence scoring

**Crates & Files**:
- `prompt-hub/src/vibe.rs` — Vibe Coding engine
- `prompt-hub/src/context_gatherer.rs` — Auto-context-gathering
- `prompt-hub/src/fallback.rs` — Auto-fallback chain
- `prompt-hub/src/preview.rs` — Auto-preview generation
- `prompt-hub/src/summarizer.rs` — Plain-English summarization
- `prompt-hub/src/confidence.rs` — Confidence scoring

**Responsibilities**:
- Implement Vibe Coding pattern matching
- Auto-gather context from codebase
- Build fallback chains for robust prompts
- Generate preview outputs
- Score confidence of results

**Git Worktree**: `worktrees/epsilon-automation/`
**Branch**: `wave9/epsilon-automation`

---

### 🔵 Zeta: Advanced Features (Tier 4+5)
**Specialty**: Evolution, cross-agent learning, cost estimation, privacy, multimodal

**Crates & Files**:
- `prompt-hub/src/evolution.rs` — Genetic algorithm for optimization
- `prompt-hub/src/pollination.rs` — Cross-agent pattern sharing
- `prompt-hub/src/tokens.rs` — Token counting and cost estimation
- `prompt-hub/src/i18n.rs` — Internationalization
- `prompt-hub/src/multimodal.rs` — Multi-modal prompt support
- `prompt-hub/src/privacy.rs` — Privacy scanning for secrets/PII
- `prompt-hub/src/quality_gate.rs` — Quality gate checks
- `prompt-hub/src/rollback.rs` — Safe deployment with auto-rollback
- `prompt-hub/src/multimodal_input.rs` — Voice/screenshot/file processing
- `prompt-hub/src/learn.rs` — Auto-learning from feedback
- `prompt-hub/src/cost.rs` — Cost tracking and estimation

**Responsibilities**:
- Implement genetic algorithms for prompt optimization
- Share patterns across agents
- Count tokens and estimate costs
- Support multiple languages
- Handle voice, images, and files as input
- Scan for privacy violations
- Gate deployments on quality metrics
- Auto-rollback failed deployments
- Learn from user feedback

**Git Worktree**: `worktrees/zeta-advanced-features/`
**Branch**: `wave9/zeta-advanced-features`

---

### 🟢 Eta: Core Hub & Metrics
**Specialty**: Central orchestration, observability, health checks, plugins

**Crates & Files**:
- `prompt-hub/src/hub.rs` — Core PromptHub orchestrator (depends on all modules)
- `prompt-hub/src/metrics.rs` — OpenTelemetry and custom metrics
- `prompt-hub/src/health.rs` — Health check aggregation
- `prompt-hub/src/shutdown.rs` — Graceful shutdown coordination
- `prompt-hub/src/plugins.rs` — Plugin system and trait definitions

**Responsibilities**:
- Coordinate all module instances
- Emit metrics and traces
- Aggregate health checks
- Coordinate graceful shutdown
- Load and manage plugins

**Git Worktree**: `worktrees/eta-hub-metrics/`
**Branch**: `wave9/eta-hub-metrics`

**Dependencies**: Depends on all other library modules; waits for their completion.

---

### 🟡 Theta: Server & CLI
**Specialty**: HTTP server, CLI interface, integration tests, examples, deployment

**Crates & Files**:
- `prompthub-server/src/main.rs` — HTTP server entry point
- `prompthub-server/src/server.rs` — Server setup and configuration
- `prompthub-server/src/routes.rs` — HTTP route handlers
- `prompthub-server/src/middleware.rs` — Request/response middleware
- `prompthub-server/src/openapi.rs` — OpenAPI specification
- `prompthub/src/main.rs` — CLI entry point
- `prompthub/src/cli.rs` — CLI argument parsing
- `prompthub/src/commands/` — Command handlers
- `tests/` — Integration tests
- `benches/` — Criterion benchmarks
- `examples/` — Working examples
- `docker/` — Docker and deployment configs

**Responsibilities**:
- Implement REST API with OpenAPI documentation
- Build CLI with intuitive command structure
- Write comprehensive integration tests
- Create examples demonstrating all features
- Dockerize the application
- Coordinate deployment workflows

**Git Worktree**: `worktrees/theta-server-cli/`
**Branch**: `wave9/theta-server-cli`

**Dependencies**: Depends on Eta (hub) which depends on all other modules.

---

## Workflow: Multi-Agent Parallel Development

### Phase 1: Setup (Sequential)
1. **Orchestrator** creates branches for all agents
2. **Orchestrator** creates worktrees for parallel work
3. All agents fetch their branch and verify setup

### Phase 2: Parallel Development
- **Alpha** designs types
- **Beta** implements storage with Alpha's types
- **Gamma** implements security using Beta's storage
- **Delta, Epsilon, Zeta** implement features in parallel (all use Alpha/Beta/Gamma)
- **Eta** waits for all to complete, then integrates
- **Theta** waits for Eta, then builds server/CLI

### Phase 3: Integration & Testing
1. All agents push their branches
2. **Orchestrator** merges sequentially (Alpha → Beta → Gamma → Delta/Epsilon/Zeta → Eta → Theta)
3. Resolve conflicts at merge boundaries
4. **Orchestrator** runs full test suite

### Phase 4: Validation
- Verify compilation with all feature combinations
- Run all integration tests
- Verify example code works
- Deploy and smoke test

---

## Setting Up Worktrees

```bash
# Create worktrees for all agents
git worktree add worktrees/alpha-core-types wave9/alpha-core-types
git worktree add worktrees/beta-storage-config wave9/beta-storage-config
git worktree add worktrees/gamma-security-audit wave9/gamma-security-audit
git worktree add worktrees/delta-search-sync wave9/delta-search-sync
git worktree add worktrees/epsilon-automation wave9/epsilon-automation
git worktree add worktrees/zeta-advanced-features wave9/zeta-advanced-features
git worktree add worktrees/eta-hub-metrics wave9/eta-hub-metrics
git worktree add worktrees/theta-server-cli wave9/theta-server-cli

# Each agent works in their worktree
cd worktrees/alpha-core-types
# ... make changes, commit
git push origin wave9/alpha-core-types

# Cleanup after merge
git worktree prune
```

---

## Coordination Rules

1. **Types flow downstream** — Alpha defines types, others use them
2. **Storage is foundation** — Beta provides storage, others build on it
3. **Security everywhere** — Gamma's auth/audit integrated into all operations
4. **No circular deps** — Verify with `cargo tree` before pushing
5. **Async only** — No blocking I/O; everything is async/await
6. **Tests required** — Every feature must have tests before merge
7. **Commit messages** — Reference the agent name and wave (e.g., "Wave9/Alpha: Define prompt models")

---

## Communication Protocol

Each agent should:
1. Update their section of `plan.md` with progress
2. Document type changes in `SESSION.md`
3. Note any blockers in `TODO.md`
4. Push commits frequently (daily minimum)
5. Review merge conflicts carefully (may indicate API mismatch)

---

## Success Criteria

- ✅ All agents complete their assigned modules
- ✅ `cargo check` passes for all feature combinations
- ✅ `cargo test --all-features` passes
- ✅ No `unsafe` code anywhere
- ✅ All clippy warnings resolved
- ✅ HTTP API documented in OpenAPI
- ✅ CLI works end-to-end
- ✅ Examples run without modification
