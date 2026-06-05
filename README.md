# PromptHub

[![CI](https://github.com/prompthub/prompthub/actions/workflows/ci.yml/badge.svg)](https://github.com/prompthub/prompthub/actions)
[![crates.io](https://img.shields.io/crates/v/prompt-hub.svg)](https://crates.io/crates/prompt-hub)
[![docs.rs](https://docs.rs/prompt-hub/badge.svg)](https://docs.rs/prompt-hub)
[![MSRV](https://img.shields.io/badge/MSRV-1.91.1-blue)](https://blog.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE-MIT)

Production-ready prompt management for LLM agent swarms. Rust 2024 Edition.

## Features

- **Vibe Coding**: Natural language to deliverable for non-technical users
- **Semantic Search**: FAST (FTS5) + SMART (ONNX embeddings) + Hybrid
- **Version Control**: Full semver lineage with diff and rollback
- **Security**: Prompt injection detection, RBAC, audit logging
- **Swarm Integration**: Role-based bundles, handoff templates
- **Enterprise**: OpenTelemetry, health checks, graceful shutdown

## Quick Start

```bash
# Install CLI
cargo install prompthub

# Initialize database
prompthub init

# Add a prompt interactively
prompthub add --interactive

# Search prompts
prompthub search "error handling" --mode hybrid

# Generate shell completions
prompthub completions bash > /etc/bash_completion.d/prompthub
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `init [path]` | Initialize a new prompt hub |
| `add [file] --interactive` | Add a new prompt |
| `get <role> <intent>` | Get a prompt for role + intent |
| `list [--domain] [--status]` | List all prompts |
| `search <query> --mode` | Search prompts (fast/smart/hybrid) |
| `update <id> <file>` | Update a prompt |
| `rollback <id> <version>` | Rollback to a previous version |
| `diff <id> <v1> <v2>` | Show diff between two versions |
| `lock <id> <ttl>` | Lock a prompt for editing |
| `unlock <token>` | Unlock a prompt |
| `audit <id>` | Show audit trail |
| `export --format <fmt> <file>` | Export prompts |
| `import <file>` | Import prompts |
| `lineage <id>` | Show prompt lineage |
| `completions <shell>` | Generate shell completions |
| `tui` | Launch TUI interface (requires `--features tui`) |
| `server --port` | Start embedded server |
| `cache <cmd>` | Cache management |
| `restore --from-backup` | Restore from backup |
| `evolve <id> --strategy` | Evolve a prompt |
| `tokens <id> --model` | Count tokens |
| `lint <file>` | Lint a prompt file |
| `plugin <cmd>` | Plugin management |
| `vibe <request>` | Vibe Coding mode |
| `magic <action> --target` | Magic Wand |
| `gather` | Gather context from current directory |
| `preview <request>` | Preview before building |
| `cost <request>` | Estimate cost |
| `scan <path>` | Scan for privacy issues |
| `deploy <id> --safe` | Deploy artifact |
| `summarize <run-id>` | Summarize a run |
| `feedback <correction>` | Submit feedback |
| `budget <cmd>` | Budget management |

## Library Usage

```rust
use prompt_hub::{PromptHub, HubConfig};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;

    // Register, search, vibe_code, etc.
    Ok(())
}
```

## Workspace

| Crate | Description |
|-------|-------------|
| `prompt-hub` | Core library |
| `prompthub` | CLI binary |
| `prompthub-server` | HTTP API server |

## Feature Flags

| Feature | Description |
|---------|-------------|
| `smart` | ONNX embedding search |
| `tui` | Terminal UI (ratatui) |
| `server` | HTTP server deps |
| `otel` | OpenTelemetry metrics |
| `vibe` | Vibe Coding mode |

## Development

```bash
# Run all tests
cargo test --all-features

# Run benchmarks
cargo bench

# Build documentation
cargo doc --all-features --no-deps

# Run with specific features
cargo run --bin prompthub --features tui -- tui
```

## Docker

```bash
docker build -f docker/Dockerfile -t prompthub .
docker-compose -f docker/docker-compose.yml up
```

## Audit Automation

The project includes an automation that triggers updates to `TODO.md` whenever a new audit report is added to
`docs/audits/`.

- **GitHub Action**: `.github/workflows/audit_sync.yml` automates this in CI.
- **Local Watcher**: `scripts/audit_watcher.sh` can be used for local development.

## Architecture

See [docs/architecture.md](docs/architecture.md) for C4 model diagrams and system design.

## License

MIT OR Apache-2.0
