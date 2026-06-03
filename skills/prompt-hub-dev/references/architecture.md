# Architecture Overview

## Workspace Structure

- `prompt-hub/`: Core library crate. Contains all business logic, storage, search, and coordination.
- `prompthub/`: CLI binary crate. Commands, TUI, and interactive features.
- `prompthub-server/`: HTTP server binary crate. Axum router, OpenAPI, and middleware.

## Core Components

- **Hub (`hub.rs`)**: Central coordinator for the library. Owns `Storage`, `SearchEngine`, `AuthManager`, etc.
- **Models (`models.rs`)**: Shared data structures. All public types must live here.
- **Storage (`storage.rs`)**: Libsql-backed persistence. All SQL lives here.
- **Search (`search.rs`)**: Multi-mode search (Fast/Smart/Hybrid).

## Dependency Rules

1.  Every module depends on `models` and `error`.
2.  `hub.rs` is the coordinator and MUST NOT be a dependency of other modules.
3.  No circular dependencies.
4.  Storage is the bottom of the stack.
