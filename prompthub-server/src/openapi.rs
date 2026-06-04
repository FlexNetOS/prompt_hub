#![forbid(unsafe_code)]

use serde_json::{Value, json};

/// OpenAPI tag constants for grouping endpoints in the generated spec.
#[cfg(feature = "utoipa")]
pub const TAG_PROMPTS: &str = "prompts";
#[cfg(feature = "utoipa")]
pub const TAG_LOCKS: &str = "locks";
#[cfg(feature = "utoipa")]
pub const TAG_AUDIT: &str = "audit";
#[cfg(feature = "utoipa")]
pub const TAG_SWARM: &str = "swarm";
#[cfg(feature = "utoipa")]
pub const TAG_HEALTH: &str = "health";
#[cfg(feature = "utoipa")]
pub const TAG_METRICS: &str = "metrics";

// ── utoipa route macro helpers ───────────────────────────────────────────

/// Re-export utoipa path macro so routes.rs can use `crate::openapi::path!`
/// without adding a direct dependency on utoipa. Scaffolded ahead of being
/// wired into routes.rs, so it is not yet referenced internally.
#[cfg(feature = "utoipa")]
#[allow(unused_imports)]
pub use utoipa::path;

/// Collect all OpenAPI paths and schemas into a single `OpenApi` instance.
#[cfg(feature = "utoipa")]
pub fn build_utoipa_spec() -> utoipa::openapi::OpenApi {
    use utoipa::OpenApi;

    #[derive(OpenApi)]
    #[openapi(
        info(
            title = "PromptHub API",
            version = env!("CARGO_PKG_VERSION"),
            description = "Production-ready prompt management for LLM agent swarms"
        ),
        servers(
            (url = "http://localhost:8080", description = "Local development")
        ),
        paths(),
        components(
            schemas(
                crate::responses::ApiResponseDoc,
                crate::responses::ErrorResponse,
            )
        ),
        tags(
            (name = "prompts", description = "Prompt CRUD operations"),
            (name = "locks", description = "Lock management for concurrent editing"),
            (name = "audit", description = "Audit trail queries"),
            (name = "swarm", description = "Swarm bundle generation"),
            (name = "health", description = "Health and probe endpoints"),
            (name = "metrics", description = "Prometheus metrics"),
        )
    )]
    struct ApiDoc;

    ApiDoc::openapi()
}

/// Convert the utoipa spec to a JSON `Value` for the handler.
#[cfg(feature = "utoipa")]
pub fn build_openapi_spec() -> Value {
    let spec = build_utoipa_spec();
    serde_json::to_value(spec).unwrap_or_else(|_| fallback_spec())
}

/// Manual fallback spec used when `utoipa` feature is disabled.
#[cfg(not(feature = "utoipa"))]
pub fn build_openapi_spec() -> Value {
    fallback_spec()
}

/// Shared fallback / base OpenAPI 3.0.3 specification.
fn fallback_spec() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "PromptHub API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Production-ready prompt management for LLM agent swarms"
        },
        "servers": [
            { "url": "http://localhost:8080", "description": "Local development" }
        ],
        "paths": {
            "/api/v1/prompts": {
                "post": {
                    "summary": "Register a new prompt",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/Prompt" }
                            }
                        }
                    },
                    "responses": {
                        "201": { "description": "Prompt created" },
                        "400": { "description": "Validation error" },
                        "401": { "description": "Unauthorized" }
                    }
                },
                "get": {
                    "summary": "List prompts",
                    "parameters": [
                        { "name": "page", "in": "query", "schema": { "type": "integer", "default": 1 } },
                        { "name": "per_page", "in": "query", "schema": { "type": "integer", "default": 20 } }
                    ],
                    "responses": {
                        "200": { "description": "Paginated prompt list" }
                    }
                }
            },
            "/api/v1/prompts/{id}": {
                "get": {
                    "summary": "Get prompt by ID",
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }
                    ],
                    "responses": {
                        "200": { "description": "Prompt found" },
                        "404": { "description": "Not found" }
                    }
                }
            },
            "/api/v1/prompts/search": {
                "get": {
                    "summary": "Search prompts",
                    "parameters": [
                        { "name": "q", "in": "query", "required": true, "schema": { "type": "string" } },
                        { "name": "mode", "in": "query", "schema": { "type": "string", "enum": ["fast", "smart", "hybrid"] } }
                    ],
                    "responses": {
                        "200": { "description": "Search results" }
                    }
                }
            },
            "/api/v1/prompts/{id}/lock": {
                "post": {
                    "summary": "Lock a prompt",
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } },
                        { "name": "ttl_seconds", "in": "query", "schema": { "type": "integer", "default": 300 } }
                    ],
                    "responses": {
                        "200": { "description": "Lock acquired" }
                    }
                },
                "delete": {
                    "summary": "Unlock a prompt",
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }
                    ],
                    "responses": {
                        "200": { "description": "Lock released" }
                    }
                }
            },
            "/api/v1/prompts/{id}/audit": {
                "get": {
                    "summary": "Get audit trail",
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }
                    ],
                    "responses": {
                        "200": { "description": "Audit entries" }
                    }
                }
            },
            "/health": {
                "get": {
                    "summary": "Health check",
                    "responses": {
                        "200": { "description": "Healthy", "content": { "application/json": { "schema": { "type": "object" } } } }
                    }
                }
            },
            "/ready": {
                "get": {
                    "summary": "Readiness probe",
                    "responses": {
                        "200": { "description": "Ready" },
                        "503": { "description": "Not ready" }
                    }
                }
            },
            "/live": {
                "get": {
                    "summary": "Liveness probe",
                    "responses": {
                        "200": { "description": "Alive" }
                    }
                }
            },
            "/metrics": {
                "get": {
                    "summary": "Prometheus metrics",
                    "responses": {
                        "200": { "description": "Metrics in Prometheus format" }
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "Prompt": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "version": { "type": "string" },
                        "status": { "type": "string", "enum": ["Draft", "Active", "Deprecated", "Archived", "Locked"] },
                        "system_prompt": { "type": "string" },
                        "user_template": { "type": "string" },
                        "domain": { "type": "string" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "created_at": { "type": "string", "format": "date-time" }
                    }
                },
                "ApiResponse": {
                    "type": "object",
                    "properties": {
                        "success": { "type": "boolean" },
                        "data": { "type": "object" },
                        "error": { "type": ["string", "null"] }
                    },
                    "required": ["success"]
                },
                "ErrorResponse": {
                    "type": "object",
                    "properties": {
                        "success": { "type": "boolean" },
                        "error": { "type": "string" },
                        "code": { "type": "integer" }
                    },
                    "required": ["success", "error", "code"]
                }
            }
        }
    })
}

/// Static OpenAPI JSON string embedded at compile time.
///
/// This is the fallback used by `swagger_ui()` when utoipa is unavailable.
pub const OPENAPI_SPEC: &str = include_str!(concat!(env!("OUT_DIR"), "/openapi.json"));

/// Serve OpenAPI spec as JSON.
pub async fn openapi_json() -> axum::Json<Value> {
    axum::Json(build_openapi_spec())
}

/// Serve Swagger UI HTML.
pub async fn swagger_ui() -> axum::response::Html<String> {
    let html = r#"
<!DOCTYPE html>
<html>
<head>
    <title>PromptHub API</title>
    <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script>
        SwaggerUIBundle({
            url: '/openapi.json',
            dom_id: '#swagger-ui',
            presets: [SwaggerUIBundle.presets.apis]
        });
    </script>
</body>
</html>
"#.to_string();
    axum::response::Html(html)
}
