---
name: prompt-hub-dev
description: "Specialized development workflows for the prompt_hub Rust project. Use when adding new Hub methods, CLI commands, server routes, or modules, ensuring compliance with strict Rust 2024 mandates, libsql conventions, and the #![forbid(unsafe_code)] rule."
---

# Prompt Hub Dev

## Overview

This skill guides the development of the `prompt_hub` workspace, a production-ready prompt management system. It ensures all changes adhere to the project's strict mandates: Rust 2024, `libsql`, `tracing`, and `forbid(unsafe_code)`.

## Workflow Decision Tree

1.  **Adding a Core Feature?** -> Follow [Adding a Hub Method](references/hub-method.md)
2.  **Adding a CLI Command?** -> Follow [Adding a CLI Command](references/cli-command.md)
3.  **Adding a Server Route?** -> Follow [Adding a Server Route](references/server-route.md)
4.  **Creating a New Module?** -> Follow [Module Creation](references/module-creation.md)

## Core Mandates

- **Rust 2024:** Use native `async fn` in traits.
- **Safety:** Every library module MUST start with `#![forbid(unsafe_code)]`.
- **Database:** Use `libsql` for persistence.
- **Logging:** Use `tracing` macros (`info!`, `warn!`, `error!`).
- **Errors:** `thiserror` for library, `anyhow` for binaries.

## Verification Workflow

Before concluding any task:
1.  Check for `#![forbid(unsafe_code)]` in all new/modified library files.
2.  Ensure `tracing::instrument` is used on public async methods.
3.  Verify that all new types are defined in `prompt-hub/src/models.rs`.
4.  Run the `scripts/check_safety.sh` to verify unsafe code prohibition.

## Resources

- [Architecture Overview](references/architecture.md)
- [Database Schema](references/database.md)
- [Testing Patterns](references/testing.md)
