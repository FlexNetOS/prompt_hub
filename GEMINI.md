# prompt_hub Project Instructions

This document provides foundational mandates and guidance for developing and maintaining the `prompt_hub` workspace.

## Core Mandates

- **Rust Edition:** Use Rust 2024 Edition.
- **Safety:** Every library module (`prompt-hub/src/`) MUST start with `#![forbid(unsafe_code)]`. No exceptions.
- **Async:** Use native `async fn` in traits. Do NOT use the `async_trait` crate.
- **Database:** Use `libsql` for all database operations. Do NOT use `rusqlite`.
- **Logging:** Use `tracing` macros (`info!`, `warn!`, `error!`, etc.) for all logging. Do NOT use `println!` or `eprintln!` in library or server code.
- **Error Handling:** Use `thiserror` for library errors and `anyhow` for application-level errors (CLI/Server).
- **Dependencies:** Workspace `Cargo.toml` MUST NOT have `optional = true` for dependencies. Set optional dependencies in individual crate `Cargo.toml` files.

## Architecture

- **`prompt-hub` (Library):** The core logic. No circular dependencies between modules. Every module should depend on `models` and `error`. `hub.rs` acts as the central coordinator.
- **`prompthub` (CLI):** Command-line interface built on top of the core library.
- **`prompthub-server` (Server):** HTTP API server built on top of the core library using `axum`.

## Subdirectory Instructions

- [prompt-hub Library Instructions](prompt-hub/GEMINI.md)
- [prompthub CLI Instructions](prompthub/GEMINI.md)
- [prompthub-server Server Instructions](prompthub-server/GEMINI.md)

## Coding Standards

- **Module Structure:** Follow the pattern: Logic in `src/<module>.rs`, unit tests in `#[cfg(test)] mod tests { ... }` at the bottom of the same file.
- **Types:** Define all public data structures in `prompt-hub/src/models.rs`. Derivations: `#[derive(Debug, Clone, Serialize, Deserialize)]`.
- **API Methods:** New `Hub` methods must be added to `prompt-hub/src/hub.rs`, use RBAC (`authorize_action`), and include audit logging for mutations.
- **Tracing:** Use `#[instrument(skip(self))]` on Hub and Storage methods.
- **Verification:** Use `scripts/check_safety.sh` to verify unsafe code prohibition and missing `#![forbid(unsafe_code)]` attributes.

## CI/CD & Automations

- **CI:** Full check, test (nextest), fmt, clippy, doc, coverage, docker build, and safety check.
- **Security:** Daily `cargo audit` and `cargo deny` checks.
- **Mutation:** `cargo mutants` for mutation testing on PRs.
- **Dependency Management:** Dependabot for cargo and github-actions.
