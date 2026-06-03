# Module Creation

1.  **Create File**: `prompt-hub/src/<module>.rs`.
2.  **Add Forbid Unsafe**: First line MUST be `#![forbid(unsafe_code)]`.
3.  **Declare Module**: Add `pub mod <module>;` in `prompt-hub/src/lib.rs`.
4.  **Implement Logic**: Use `tracing`, `thiserror`.
5.  **Add Tests**: `mod tests` at the bottom.
