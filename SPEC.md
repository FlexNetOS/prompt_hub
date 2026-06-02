# SPEC.md — prompt_hub Rust Workspace Specification

## Workspace Structure
```
prompt_hub/
├── Cargo.toml                    # Workspace root
├── rust-toolchain.toml           # MSRV 1.91.1
├── prompt-hub/                   # Core library crate
│   ├── Cargo.toml
│   ├── build.rs
│   └── src/
│       ├── lib.rs
│       ├── models.rs
│       ├── hub.rs
│       ├── storage.rs
│       ├── search.rs
│       ├── swarm.rs
│       ├── sanitize.rs
│       ├── auth.rs
│       ├── audit.rs
│       ├── metrics.rs
│       ├── health.rs
│       ├── sync.rs
│       ├── config.rs
│       ├── templates.rs
│       ├── defaults.rs
│       ├── error.rs
│       ├── shutdown.rs
│       ├── plugins.rs
│       ├── evolution.rs
│       ├── pollination.rs
│       ├── tokens.rs
│       ├── i18n.rs
│       ├── multimodal.rs
│       ├── vibe.rs
│       ├── context_gatherer.rs
│       ├── fallback.rs
│       ├── preview.rs
│       ├── summarizer.rs
│       ├── confidence.rs
│       ├── cost.rs
│       ├── privacy.rs
│       ├── quality_gate.rs
│       ├── rollback.rs
│       ├── multimodal_input.rs
│       └── learn.rs
├── prompthub/                    # CLI binary crate
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── cli.rs
│       ├── tui.rs
│       ├── fuzzy.rs
│       └── commands/
├── prompthub-server/             # HTTP server binary crate
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── server.rs
│       ├── routes.rs
│       ├── middleware.rs
│       └── openapi.rs
├── migrations/
│   ├── 0001_initial.sql
│   ├── 0002_audit.sql
│   ├── 0003_locks.sql
│   ├── 0004_swarm_state.sql
│   ├── 0005_backup_meta.sql
│   ├── 0006_plugins.sql
│   ├── 0007_soft_delete.sql
│   ├── 0008_generation_params.sql
│   └── 0009_config.sql
├── tests/
├── benches/
├── examples/
├── docs/
├── docker/
├── plugins/
└── README.md
```

## Tech Stack
- Rust 2024 Edition, MSRV 1.91.1
- `#![forbid(unsafe_code)]` in library crate
- libsql (NOT sqlite) for database with native vector search
- tokio 1.52.3 (full features)
- axum 0.8.8 for HTTP server
- clap 4.6.1 derive for CLI
- thiserror 2.0.12 / anyhow 1.0.98 for errors
- tracing + tracing-subscriber for logging
- serde + serde_json + serde_yaml + toml for serialization
- handlebars 6.4 (default), tera 1.20.1 (optional) for templates
- fastembed 5.15.0 + ort 2.0.15 (optional) for embeddings
- uuid 1.18 (v7), chrono 0.4.41, semver 1.0.26

## Feature Flags
- smart: ONNX embedding search
- tui: Terminal UI (ratatui)
- server: HTTP API server dependencies
- otel: OpenTelemetry metrics
- sqlcipher: Encryption at rest
- tls: TLS/mTLS support
- ffi: C ABI bindings (cbindgen)
- handlebars: Handlebars template engine (default)
- tera: Tera template engine
- tiktoken: Token counting
- tokenizers: HuggingFace tokenizers
- plugins: Dynamic plugin loading
- qdrant: Qdrant vector backend
- vibe: Full Vibe Coding mode
- And all Tier 5 automation feature flags

## Key Types (models.rs)
- Prompt, PromptVersion, PromptMetrics, SwarmBundle, AgentIdentity
- GenerationParams, SearchMode, Status, Role, Domain, Capability, Conflict
- VibeResult, CostEstimate, ConfidenceScore, UserInput, PrivacyReport
- QualityResult, DeployResult, UserCorrection, ContentModerator
- LLMProvider, BudgetManager, CostLimiter, TokenQuota, CostAlert
- CanaryDeployment, GradualRollout, BetaUserProgram
- All structs derive: Debug, Clone, Serialize, Deserialize where applicable
- All use native async fn in traits (Rust 2024 Edition, no async_trait crate)

## Hub API (hub.rs)
- PromptHub::new() -> Send + Sync + 'static
- register(), get(), update(), rollback(), search(), lock(), unlock(), audit_trail()
- transfer_ownership(), evolve_prompt(), count_tokens()
- vibe_code(), gather_context(), generate_preview(), estimate_cost()
- scan_privacy(), run_quality_gate(), deploy_safe(), summarize_result()
- score_confidence(), fallback_chain(), learn_from_feedback(), multimodal_process()

## Database Schema (libsql)
- prompts: id, name, version, status, system_prompt, user_template, required_vars (JSON),
  domain, tags (JSON), target_roles (JSON), metadata (JSON), metrics (JSON),
  author_id, created_at, updated_at, deleted_at (soft delete),
  generation_params (JSON), locale, multimodal_config (JSON)
- versions: prompt_id, parent_id, version, changelog, diff, created_at
- metrics: prompt_id, usage_count, success_rate, avg_tokens, avg_latency_ms, last_used, cost_estimate_usd
- embeddings: prompt_id, embedding (F32_BLOB(384))
- locks: id, prompt_id, agent_id, token_hash, expires_at, created_at
- audit_log: id, timestamp, agent_id, action, prompt_id, diff_hash, before_json, after_json, ip_address
- agents: id, name, capabilities, token_hash, specialization_score, created_at
- swarm_state: id, from_state, to_state, trigger_agent, reason, created_at
- config: key, value, updated_at
- plugin_registry: id, name, version, path, enabled, health_status, created_at
- prompts_fts: FTS5 virtual table (name, system_prompt, tags)

## Search (search.rs)
- SearchEngine trait with FastEngine (FTS5), SmartEngine (ONNX embeddings), HybridEngine
- PluginEngine for delegated backends
- Cache eviction: LRU + TTL for embeddings

## Auth (auth.rs)
- AgentIdentity with argon2id hashed tokens, specialization scoring
- Capability: Read, Write, Admin, SwarmOnly
- AuthManager trait: authenticate(), authorize()

## Sanitize (sanitize.rs)
- 5+ heuristics: system prompt leakage, jailbreak patterns, delimiter injection,
  variable injection, encoding obfuscation
- SanitizationResult: Clean, Suspicious(Vec<Issue>), Blocked(Vec<Issue>)

## CLI (prompthub/src/cli.rs)
- 15+ commands: Init, Add, Get, List, Search, Update, Rollback, Diff, Lock, Unlock,
  Audit, Export, Import, Lineage, Completions, Tui, Server, Cache, Restore,
  Evolve, Tokens, Lint, Plugin, Vibe, Magic, Voice, Onboard, Heal, Suggest,
  Preview, Cost, Scan, Gather, Deploy, Summarize, Feedback, Multimodal, Budget, Quota

## Server (prompthub-server/src/)
- axum 0.8.8 router: CRUD, search, lock, audit, health, metrics, ready, live
- utoipa 5.0.1 OpenAPI generation
- tower-governor 0.7.0 rate limiting
- tower-http 0.6.2 middleware: CORS, compression, request ID
- TLS/mTLS support (optional feature)
- Graceful shutdown with 30s drain

## Testing
- 90%+ coverage on prompt-hub/src/
- Unit tests in each module file
- Integration tests in tests/ directory
- Benchmarks in benches/ directory (Criterion)
- Examples in examples/ directory

## CI/CD (.github/workflows/ci.yml)
- Matrix: ubuntu, macos, windows; Rust 1.96.0 and 1.91.1
- cargo check/test/clippy/doc/tarpaulin/nextest/mutants/publish dry-run

## Docker
- Multi-stage build with distroless final image (<50MB)
- docker-compose.yml with health checks

## Documentation
- README.md with badges, quickstart, examples
- docs/adr/ - 15 Architecture Decision Records
- docs/architecture.md - C4 model diagrams
- docs/deployment.md - Blue/green strategy
- docs/runbooks/ - Operational procedures
