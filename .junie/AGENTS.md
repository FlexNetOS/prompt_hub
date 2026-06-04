# Project Overview: PromptHub

PromptHub is a production-ready prompt management system for LLM agent swarms, built with Rust (2024 Edition). It
provides a central repository for managing, versioning, and searching prompts with a focus on performance, security, and
integration with agentic workflows.

## Core Components

The project is structured as a Rust workspace with three primary crates:

- **`prompt-hub`**: The core library containing the business logic, storage abstraction (libsql), search engines, and
  swarm integration features.
- **`prompthub`**: A feature-rich CLI tool for interacting with the hub, including a TUI mode and support for "Vibe
  Coding".
- **`prompthub-server`**: An HTTP API server built with Axum, providing remote access to prompt management features with
  OpenAPI support and rate limiting.

## Key Features

- **Semantic Search**: Uses a hybrid approach combining FTS5 (Fast) and ONNX embeddings (Smart) via `fastembed`.
- **Vibe Coding**: A natural language interface to generate deliverables from high-level requests.
- **Version Control**: Full semver-based versioning for prompts with diffing, rollback capabilities, and audit logging.
- **Swarm Integration**: Role-based prompt bundles and handoff templates designed for multi-agent systems.
- **Security & Reliability**: Prompt injection detection, RBAC, audit trails, and OpenTelemetry integration.
- **Multimodal Support**: Framework for handling diverse input types beyond just text.

## Tech Stack

- **Language**: Rust 2024 Edition (MSRV 1.91.1)
- **Runtime**: Tokio
- **Database**: libsql (supporting native vector search)
- **Web Framework**: Axum (for the server)
- **CLI Framework**: Clap
- **Template Engines**: Handlebars (default) and Tera
- **Observability**: Tracing + OpenTelemetry

## Workspace Structure

- `prompt-hub/`: Core library logic.
- `prompthub/`: CLI implementation.
- `prompthub-server/`: REST API implementation.
- `migrations/`: SQL migration files for the database schema.
- `docs/`: Architecture diagrams (C4 model), ADRs, and deployment guides.
- `tests/` & `benches/`: Integration tests and performance benchmarks.

## Build/Configuration Instructions

### Environment Setup

- **Rust Toolchain**: Ensure you are using Rust 2024 edition (MSRV 1.91.1). `rust-toolchain.toml` is present to manage
  this automatically.
- **Dependencies**: The project requires `libsql`. For local development, an in-memory or local file-based database is
  typically used.
- **Environment Variables**:
  - `DATABASE_URL`: Path to the libsql database (e.g., `file:prompthub.db` or `:memory:`).
  - `RUST_LOG`: Set to `info` or `debug` for tracing output.

### Build Commands

- **Full Workspace**: `cargo build --workspace`
- **Specific Crate**: `cargo build -p prompt-hub`
- **Release Build**: `cargo build --release` (includes LTO and stripped symbols)

### Docker Build

- Use the provided multi-stage Dockerfiles in `docker/` for production-ready distroless images.

## Testing Information

### Running Tests

- **All Tests**: `cargo test --all-features`
- **Core Library Tests**: `cargo test -p prompt-hub`
- **CLI Tests**: `cargo test -p prompthub`
- **Server Tests**: `cargo test -p prompthub-server`
- **Individual Test File**: `cargo test -p prompt-hub --test <test_name>`

### Adding New Tests

- **Unit Tests**: Place in a `mod tests` block at the bottom of the relevant `.rs` file.
- **Integration Tests**: Place in the `tests/` directory of the respective crate.
- **Guidelines**:
  - Use `tokio::test` for async tests.
  - Mock external services where possible.
  - Ensure tests are idempotent and don't leave side effects in the environment.

### Demonstration Test

Here is a simple test case for creating a prompt, which you can find in `prompt-hub/tests/test_demo.rs`:

```rust
use prompt_hub::models::Prompt;

#[test]
fn test_demonstration_prompt_creation() {
  let name = "test-prompt";
  let system_prompt = "You are a helpful assistant.";
  let prompt = Prompt::new(name, system_prompt);

  assert_eq!(prompt.name, name);
  assert_eq!(prompt.system_prompt, system_prompt);
  assert_eq!(prompt.version.to_string(), "0.1.0"); // Default version
}
```

## Additional Development Information

### Code Style & Standards

- **Linter**: Use `cargo clippy` locally. Qodana is used for CI/CD quality gates.
- **Formatting**: `cargo fmt` (standard Rust style).
- **Safety**: `#![forbid(unsafe_code)]` is enforced in the core library.
- **Documentation**: Use triple-slash `///` for public API documentation. Use `cargo doc --open` to view.

### Development Utilities

- **Justfile**: Check `justfile` in the root for common task shortcuts.
- **Templates**: Handlebars and Tera are supported for prompt templating.

## Production Code Standard

To achieve and maintain production-grade quality, follow these guidelines:

### 1. Dependency Management

- **Keep Dependencies Lean**: Periodically check for unused crates (e.g., `serde_yaml`, `dashmap`, `insta` were flagged
  in recent audits).
- **Update Regularly**: Monitor for newer versions of critical crates like `tokio`, `axum`, and `opentelemetry`.

### 2. Code Quality & Refactoring

- **Idiomatic Rust**:
  - Prefer `assert_eq!(a, b)` over `assert!(a == b)`.
  - Lift `return` keywords out of `if` blocks where possible.
  - Remove unnecessary path prefixes.
- **Error Handling**: Use the crate-specific `Result` and `HubError` types. Avoid `unwrap()` in production code.

### 3. Observability & Security

- **Tracing**: Instrument all public methods with `#[instrument]`.
- **Injection Detection**: Always validate prompt inputs against injection patterns.
- **RBAC**: Ensure all API endpoints in `prompthub-server` have proper role-based access control checks.

### 4. Path to Production Grade

- **Current Status**: Functional core with comprehensive model scaffolding.
- **Gap to Production**:
  - Address all "Unused crate" warnings from Qodana.
  - Implement full audit trail persistence in all storage operations.
  - Complete the implementation of todos in `prompt-hub/src/search.rs` regarding native vector search.
  - Increase test coverage for `swarm` and `vibe` modules.
