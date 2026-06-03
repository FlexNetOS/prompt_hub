# prompthub-server Instructions

This document provides scoped instructions for the `prompthub-server` HTTP API application.

## Server Architecture

- **Framework:** Built with `axum`.
- **State Management:** Uses `Arc<AppState>` to share the `PromptHub` instance across handlers.
- **Documentation:** Uses `utoipa` for automatic OpenAPI (Swagger) specification generation.
- **Middleware:** Uses `tower-http` for CORS, compression, and request ID tracking.

## Adding a New Route

1. Define the route in `prompthub-server/src/server.rs` within the `create_router` function.
2. Implement the handler function in `prompthub-server/src/routes.rs`.
3. Handler signature: `pub async fn <handler_name>(State(state): State<Arc<AppState>>, ...) -> impl IntoResponse`.
4. Return `(StatusCode, Json<ApiResponse<T>>)` using the `success()` or `error()` helpers.
5. Add `utoipa` macros (`#[utoipa::path(...)]`) to the handler for OpenAPI generation.
6. Update the `ApiDoc` struct in `prompthub-server/src/openapi.rs` to include the new handler.

## Standards

- **Graceful Shutdown:** The server must support graceful shutdown (30s drain period).
- **Rate Limiting:** Apply rate limiting where appropriate using `tower-governor`.
- **JSON Serialization:** Use `serde_json` for all API responses.
