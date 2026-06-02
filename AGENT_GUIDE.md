# Agent Guide — Navigating prompt_hub

> Quick reference for AI agents. Read this first when resuming work.

## Find Anything in 3 Seconds

| Looking for | File | Grep pattern |
|-------------|------|-------------|
| **Any type definition** (struct, enum) | `prompt-hub/src/models.rs` | `pub struct <Name>` / `pub enum <Name>` |
| **Any Hub API method** | `prompt-hub/src/hub.rs` | `pub async fn <method>` |
| **Database queries** | `prompt-hub/src/storage.rs` | `conn.execute(` / `conn.query(` |
| **FTS5 search logic** | `prompt-hub/src/search.rs` | `prompts_fts MATCH` |
| **Vector similarity** | `prompt-hub/src/search.rs` | `cosine_similarity` |
| **Auth/RBAC** | `prompt-hub/src/auth.rs` | `authorize_action` |
| **Prompt sanitization** | `prompt-hub/src/sanitize.rs` | `detect_` |
| **Vibe Coding engine** | `prompt-hub/src/vibe.rs` | `vibe_code` |
| **Audit logging** | `prompt-hub/src/audit.rs` | `log(` / `fetch_audit` |
| **Lock management** | `prompt-hub/src/lock.rs` | `create_lock` |
| **Template rendering** | `prompt-hub/src/templates.rs` | `render(` |
| **Config loading** | `prompt-hub/src/config.rs` | `HubConfig` |
| **Any CLI command** | `prompthub/src/cli.rs` | `Commands::` |
| **CLI handler** | `prompthub/src/commands/<cmd>.rs` | `pub async fn run` |
| **HTTP route handler** | `prompthub-server/src/routes.rs` | `pub async fn <handler>` |
| **Middleware** | `prompthub-server/src/middleware.rs` | `async fn` |
| **OpenAPI spec** | `prompthub-server/src/openapi.rs` | `build_openapi_spec` |
| **Error types** | `prompt-hub/src/error.rs` | `pub enum HubError` |
| **SQL table schema** | `migrations/0001_initial.sql` | `CREATE TABLE` |
| **Feature flag** | `prompt-hub/Cargo.toml` | `features` section |
| **Module declaration** | `prompt-hub/src/lib.rs` | `pub mod` |

## Architecture at a Glance

```
[prompthub CLI] ──calls──→ [prompt-hub lib] ←──calls── [prompthub-server HTTP]
                                │
                    ┌───────────┼───────────┐
                    ▼           ▼           ▼
               [libsql DB]  [FTS5/]    [in-memory]
               (storage.rs)  [embed]    (sync/events)
                           (search.rs)
```

## Module Dependency Rules

1. **Every module depends on `models` and `error`** — these are the foundation
2. **`hub.rs` is the coordinator** — it owns instances of all other modules but modules don't depend on `hub`
3. **No circular deps** — if A imports B, B never imports A
4. **Storage is the bottom of the stack** — everything above it calls into it
5. **Feature flags are per-crate** — workspace Cargo.toml has NO `optional = true` (was a parse error)

## Adding a New Module

1. Create `prompt-hub/src/<module>.rs` with `#![forbid(unsafe_code)]`
2. Add `pub mod <module>;` to `prompt-hub/src/lib.rs` (alphabetical order)
3. Write real logic (no stubs, no `todo!()`)
4. Add `#[cfg(test)] mod tests { ... }` at the bottom
5. If the module has public types, re-export in `hub.rs` if needed
6. Add any new types to `models.rs` (not in the module file)

## Adding a New Type

1. Add to `prompt-hub/src/models.rs` with `#[derive(Debug, Clone, Serialize, Deserialize)]`
2. Use `pub struct` or `pub enum`
3. Add `Default` impl if used by tests or CLI
4. Types are referenced by `crate::models::TypeName` or `super::TypeName`

## Adding a New Hub Method

1. Add `pub async fn <name>(&self, ...) -> Result<T>` to `prompt-hub/src/hub.rs`
2. Use `self.auth.authorize_action(identity, Action::Read)?` for RBAC
3. Call `self.storage.<method>().await?` for database operations
4. Add `#[instrument(skip(self))]` for tracing
5. Add audit logging after mutating operations

## Adding a New HTTP Route

1. Add route to `prompthub-server/src/server.rs` `create_router()`
2. Add handler to `prompthub-server/src/routes.rs`
3. Handler signature: `pub async fn handler(State(state): State<Arc<AppState>>, ...) -> impl IntoResponse`
4. Call `state.hub.<method>().await` for real logic
5. Return `(StatusCode, Json<ApiResponse<T>>)` using `success()` or `error()` helpers

## Adding a New CLI Command

1. Add variant to `Commands` enum in `prompthub/src/cli.rs`
2. Add `use prompt_hub::models::*;` imports
3. Add handler arm in `prompthub/src/main.rs` `match args.command`
4. Or add handler file in `prompthub/src/commands/<cmd>.rs`

## Test Patterns

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // Arrange
        let input = ...;
        // Act
        let result = something(input);
        // Assert
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_async_something() {
        let result = something_async().await;
        assert!(result.is_ok());
    }
}
```

## Common Gotchas

- **Rust 2024 Edition** — use native `async fn` in traits, NOT `async_trait` crate
- **`#![forbid(unsafe_code)]`** — every library .rs file must have this as first line
- **`optional = true` NOT allowed in `[workspace.dependencies]`** — set it in individual crate Cargo.toml files
- **libsql, not sqlite** — use `libsql::Connection`, not `rusqlite::Connection`
- **All types must be Send + Sync** — use `Arc<T>` for shared state, not `Rc<T>`
- **Semver parsing can fail** — always use `.unwrap_or(Version::new(0,0,0))` on parse
- **JSON column parsing** — use `serde_json::from_str(s).unwrap_or_default()` for DB JSON columns
- **No `println!` in library code** — use `tracing::info!()` / `tracing::warn!()` / `tracing::error!()`

## Files to Read First When Resuming

1. `SESSION.md` — what was last done, what's pending
2. `TODO.md` — prioritized action items
3. `prompt-hub/src/lib.rs` — module declarations (know what's available)
4. `prompt-hub/src/hub.rs` — the main API (know what methods exist)
5. `prompt-hub/src/models.rs` — types (know the data model)
6. `Cargo.toml` — workspace structure and dependencies
