# prompt-hub Library Instructions

This document provides scoped instructions for the `prompt-hub` core library crate.

## Module Dependency Rules

1. **Foundations:** Every module depends on `models` and `error`. These are the base of the library.
2. **Coordinator:** `hub.rs` is the coordinator. It owns instances of other modules, but modules MUST NOT depend on `hub.rs`.
3. **Circular Dependencies:** No circular dependencies are allowed. If module A imports module B, module B must never import module A.
4. **Storage:** `storage.rs` is at the bottom of the stack. Most other modules will call into it for data persistence.

## Adding a New Module

1. Create `prompt-hub/src/<module>.rs`.
2. Add `#![forbid(unsafe_code)]` as the first line of the file.
3. Add `pub mod <module>;` to `prompt-hub/src/lib.rs` in alphabetical order.
4. Implement logic without stubs or `todo!()`.
5. Add unit tests in a `mod tests` block at the bottom of the file.
6. Re-export public types in `hub.rs` if they are part of the main API.

## Adding a New Hub Method

1. Define the method in `prompt-hub/src/hub.rs`: `pub async fn <name>(&self, ...) -> Result<T, HubError>`.
2. Implement RBAC check: `self.auth.authorize_action(identity, Action::<ActionName>)?`.
3. Call the appropriate `storage` or module method.
4. Add `#[instrument(skip(self))]` for tracing.
5. Add audit logging for any mutating operations: `self.storage.log_audit(...).await?`.

## Database Conventions

- All SQL queries should be located in `prompt-hub/src/storage.rs`.
- Use `libsql` features for vector search and FTS5.
- Handle JSON columns by parsing into Serde-compatible types defined in `models.rs`.
