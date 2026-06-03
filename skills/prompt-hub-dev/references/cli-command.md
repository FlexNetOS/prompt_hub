# Adding a CLI Command

1.  **Define Command**: Add a new variant to `Commands` enum in `prompthub/src/cli.rs`.
2.  **Add Handler**:
    -   Optionally create `prompthub/src/commands/<cmd>.rs`.
    -   Implement the `run` function.
3.  **Register Handler**: Add a match arm in `prompthub/src/main.rs`.
4.  **Integration**: Ensure it uses `PromptHub` for logic.
