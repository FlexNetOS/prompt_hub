# prompthub CLI Instructions

This document provides scoped instructions for the `prompthub` CLI application.

## CLI Development Guidelines

- **Argument Parsing:** Use `clap` with the `derive` feature for all command-line argument parsing.
- **Error Handling:** Use `anyhow::Result` for the `main` function and command handlers to provide context-rich error messages.
- **Interacting with the Hub:** The CLI should primarily interact with the `prompt-hub` library through the `PromptHub` struct.

## Adding a New Command

1. Add a new variant to the `Commands` enum in `prompthub/src/cli.rs`.
2. Define any necessary subcommands or flags using `clap` attributes.
3. (Optional) Create a new handler file in `prompthub/src/commands/<cmd>.rs` if the logic is complex.
4. Add a new match arm in `prompthub/src/main.rs` to handle the command.
5. Ensure the command handler calls the appropriate method on `PromptHub`.

## Terminal UI (TUI)

- The TUI is an optional feature (`--features tui`) built with `ratatui`.
- TUI-specific logic should be contained within `prompthub/src/tui.rs`.
