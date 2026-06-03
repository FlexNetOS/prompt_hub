# Adding a Server Route

1.  **Define Route**: In `prompthub-server/src/server.rs`, add the route to `create_router`.
2.  **Implement Handler**: In `prompthub-server/src/routes.rs`, add the handler function.
3.  **OpenAPI**: Add `#[utoipa::path(...)]` and update `ApiDoc` in `openapi.rs`.
