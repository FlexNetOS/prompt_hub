#![forbid(unsafe_code)]

use serde_json::Value;

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
    const FALLBACK_SPEC_JSON: &str = r##"""
{
        "openapi": "3.0.3",
        "info": {
            "title": "PromptHub API",
            "version": "{VERSION}",
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
                },
                "patch": {
                    "summary": "Partially update a prompt",
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/UpdatePromptRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Prompt updated" },
                        "400": { "description": "Bad request" },
                        "403": { "description": "Forbidden" },
                        "404": { "description": "Not found" },
                        "422": { "description": "Validation error" }
                    }
                }
            },
            "/api/v1/prompts/get": {
                "get": {
                    "summary": "Get best matching prompt by role and intent",
                    "parameters": [
                        { "name": "role", "in": "query", "required": true, "schema": { "type": "string" } },
                        { "name": "intent", "in": "query", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": { "description": "Prompt found" },
                        "400": { "description": "Bad request" },
                        "404": { "description": "No matching prompt" }
                    }
                }
            },
            "/api/v1/prompts/{id}/rollback": {
                "post": {
                    "summary": "Roll back a prompt to a previous version",
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/RollbackRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Prompt rolled back" },
                        "400": { "description": "Bad request" },
                        "403": { "description": "Forbidden" },
                        "404": { "description": "Not found" }
                    }
                }
            },
            "/api/v1/prompts/{id}/transfer": {
                "post": {
                    "summary": "Transfer prompt ownership",
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/TransferOwnershipRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Ownership transferred" },
                        "400": { "description": "Bad request" },
                        "403": { "description": "Forbidden" },
                        "404": { "description": "Not found" }
                    }
                }
            },
            "/api/v1/seed": {
                "post": {
                    "summary": "Seed default prompt templates",
                    "responses": {
                        "200": { "description": "Templates seeded" }
                    }
                }
            },
            "/api/v1/template/lint": {
                "post": {
                    "summary": "Lint a raw template string",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/LintTemplateRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Lint results" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/api/v1/cost/estimate": {
                "post": {
                    "summary": "Estimate cost for an intent",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/EstimateCostRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Cost estimate" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/api/v1/learn": {
                "post": {
                    "summary": "Record feedback for learning",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/LearnFeedbackRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Feedback recorded" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/api/v1/confidence": {
                "post": {
                    "summary": "Score confidence for an intent",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ScoreConfidenceRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Confidence score" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/api/v1/privacy/scan": {
                "post": {
                    "summary": "Scan text for privacy violations",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ScanPrivacyRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Privacy report" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/api/v1/fallback": {
                "post": {
                    "summary": "Execute fallback chain for an intent",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/FallbackChainRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Fallback artifact" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/api/v1/context/gather": {
                "post": {
                    "summary": "Gather full project context",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/GatherContextRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Project context" },
                        "400": { "description": "Bad request" },
                        "500": { "description": "Gather failed" }
                    }
                }
            },
            "/api/v1/context/gather/smart": {
                "post": {
                    "summary": "Gather smart project context with relevance and patterns",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/GatherContextSmartRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Smart project context" },
                        "400": { "description": "Bad request" },
                        "500": { "description": "Gather failed" }
                    }
                }
            },
            "/api/v1/context/files": {
                "post": {
                    "summary": "Collect relevance-ranked files",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CollectRelevantFilesRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Ranked file list" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/api/v1/context/patterns": {
                "post": {
                    "summary": "Extract structural code patterns",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ExtractPatternsRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Extracted patterns" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/api/v1/lineage/ancestry/{version_id}": {
                "get": {
                    "summary": "Get lineage ancestry for a version",
                    "parameters": [
                        { "name": "version_id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": { "description": "Ancestry path" },
                        "404": { "description": "Version not found" }
                    }
                }
            },
            "/api/v1/lineage/forks": {
                "get": {
                    "summary": "Detect all lineage forks",
                    "responses": {
                        "200": { "description": "Fork list" }
                    }
                }
            },
            "/api/v1/lineage/descendants/{version_id}": {
                "get": {
                    "summary": "Get descendants of a version",
                    "parameters": [
                        { "name": "version_id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": { "description": "Descendant version IDs" }
                    }
                }
            },
            "/api/v1/lineage/tree/{version_id}": {
                "get": {
                    "summary": "Build lineage tree from a root version",
                    "parameters": [
                        { "name": "version_id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": { "description": "Lineage tree" },
                        "404": { "description": "Version not found" }
                    }
                }
            },
            "/api/v1/lineage/count": {
                "get": {
                    "summary": "Count lineage nodes",
                    "responses": {
                        "200": { "description": "Node count" }
                    }
                }
            },
            "/api/v1/lineage/roots": {
                "get": {
                    "summary": "List root versions",
                    "responses": {
                        "200": { "description": "Root version IDs" }
                    }
                }
            },
            "/api/v1/lineage/has/{version_id}": {
                "get": {
                    "summary": "Check whether a version is tracked",
                    "parameters": [
                        { "name": "version_id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": { "description": "Has-version flag" }
                    }
                }
            },
            "/api/v1/providers/register": {
                "post": {
                    "summary": "Register a provider for health monitoring",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/RegisterProviderRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Provider registered" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/api/v1/providers/{name}/success": {
                "post": {
                    "summary": "Record a successful provider probe",
                    "parameters": [
                        { "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/RecordProviderSuccessRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Success recorded" }
                    }
                }
            },
            "/api/v1/providers/{name}/failure": {
                "post": {
                    "summary": "Record a failed provider probe",
                    "parameters": [
                        { "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": { "description": "Failure recorded" }
                    }
                }
            },
            "/api/v1/providers/{name}/healthy": {
                "get": {
                    "summary": "Check whether a provider is healthy",
                    "parameters": [
                        { "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": { "description": "Health flag" }
                    }
                }
            },
            "/api/v1/providers/health": {
                "get": {
                    "summary": "Get provider health summary",
                    "responses": {
                        "200": { "description": "Health summary" }
                    }
                }
            },
            "/api/v1/multi-provider/providers": {
                "post": {
                    "summary": "Add a provider to the multi-provider routing pool",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/AddMultiProviderRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Provider added" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/api/v1/multi-provider/route": {
                "get": {
                    "summary": "Select the best provider for routing",
                    "parameters": [
                        { "name": "vendor", "in": "query", "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": { "description": "Routing decision" },
                        "503": { "description": "No healthy provider" }
                    }
                }
            },
            "/api/v1/multi-provider/providers/{name}/success": {
                "post": {
                    "summary": "Record a successful multi-provider request",
                    "parameters": [
                        { "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": { "description": "Success recorded" }
                    }
                }
            },
            "/api/v1/multi-provider/providers/{name}/failure": {
                "post": {
                    "summary": "Record a failed multi-provider request",
                    "parameters": [
                        { "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": { "description": "Failure recorded" }
                    }
                }
            },
            "/api/v1/multi-provider/stats": {
                "get": {
                    "summary": "Get multi-provider pool statistics",
                    "responses": {
                        "200": { "description": "Pool statistics" }
                    }
                }
            },
            "/api/v1/rollouts/check": {
                "post": {
                    "summary": "Check whether a user is included in a canary rollout",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CheckRolloutRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Inclusion flag" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/api/v1/rollouts": {
                "post": {
                    "summary": "Register a graduated rollout configuration",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/RegisterRolloutRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Rollout registered" }
                    }
                }
            },
            "/api/v1/rollouts/inclusion": {
                "post": {
                    "summary": "Find rollout inclusion for a user",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/FindRolloutInclusionRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Inclusion result" },
                        "400": { "description": "Bad request" }
                    }
                }
            },
            "/api/v1/rollouts/evaluate-rollback": {
                "post": {
                    "summary": "Evaluate auto-rollback for a rollout",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/EvaluateAutoRollbackRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Rollback evaluation" },
                        "404": { "description": "Rollout not found" }
                    }
                }
            },
            "/api/v1/rollouts/advance": {
                "post": {
                    "summary": "Advance a rollout segment to the next stage",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/AdvanceSegmentRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Advanced stage" },
                        "404": { "description": "Rollout or segment not found" }
                    }
                }
            },
            "/api/v1/deploy": {
                "post": {
                    "summary": "Deploy an artifact with optional rollback",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/DeployWithRollbackRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Deployment result" },
                        "500": { "description": "Deployment failed" }
                    }
                }
            },
            "/api/v1/rollback/{id}/restore": {
                "post": {
                    "summary": "Restore a snapshot by ID",
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": { "description": "Snapshot restored" },
                        "404": { "description": "Snapshot not found" },
                        "500": { "description": "Restore failed" }
                    }
                }
            },
            "/api/v1/rollback/{id}/available": {
                "get": {
                    "summary": "Check whether a rollback snapshot is available",
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": { "description": "Availability flag" }
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
                },
                "UpdatePromptRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "system_prompt": { "type": "string" },
                        "user_template": { "type": "string" },
                        "required_vars": { "type": "array", "items": { "type": "string" } },
                        "domain": { "type": "string" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "target_roles": { "type": "array", "items": { "type": "string" } },
                        "status": { "type": "string" }
                    }
                },
                "RollbackRequest": {
                    "type": "object",
                    "properties": {
                        "to_version": { "type": "string" }
                    },
                    "required": ["to_version"]
                },
                "TransferOwnershipRequest": {
                    "type": "object",
                    "properties": {
                        "to_agent_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["to_agent_id"]
                },
                "FallbackChainRequest": {
                    "type": "object",
                    "properties": {
                        "intent_text": { "type": "string" },
                        "project_path": { "type": "string" }
                    },
                    "required": ["intent_text"]
                },
                "LearnFeedbackRequest": {
                    "type": "object",
                    "properties": {
                        "correction": { "type": "string" },
                        "intent_text": { "type": "string" },
                        "agent_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["correction", "intent_text", "agent_id"]
                },
                "ScoreConfidenceRequest": {
                    "type": "object",
                    "properties": {
                        "intent_text": { "type": "string" },
                        "project_path": { "type": "string" }
                    },
                    "required": ["intent_text"]
                },
                "ScanPrivacyRequest": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" }
                    },
                    "required": ["text"]
                },
                "EstimateCostRequest": {
                    "type": "object",
                    "properties": {
                        "intent_text": { "type": "string" },
                        "project_path": { "type": "string" }
                    },
                    "required": ["intent_text"]
                },
                "LintTemplateRequest": {
                    "type": "object",
                    "properties": {
                        "template": { "type": "string" }
                    },
                    "required": ["template"]
                },
                "GatherContextRequest": {
                    "type": "object",
                    "properties": {
                        "project_path": { "type": "string" }
                    },
                    "required": ["project_path"]
                },
                "GatherContextSmartRequest": {
                    "type": "object",
                    "properties": {
                        "project_path": { "type": "string" }
                    },
                    "required": ["project_path"]
                },
                "CollectRelevantFilesRequest": {
                    "type": "object",
                    "properties": {
                        "project_path": { "type": "string" }
                    },
                    "required": ["project_path"]
                },
                "ExtractPatternsRequest": {
                    "type": "object",
                    "properties": {
                        "project_path": { "type": "string" }
                    },
                    "required": ["project_path"]
                },
                "RegisterProviderRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "url": { "type": "string" }
                    },
                    "required": ["name", "url"]
                },
                "RecordProviderSuccessRequest": {
                    "type": "object",
                    "properties": {
                        "latency_ms": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["latency_ms"]
                },
                "AddMultiProviderRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "vendor": { "type": "string" },
                        "endpoint": { "type": "string" },
                        "priority": { "type": "integer", "minimum": 0 },
                        "max_retries": { "type": "integer", "minimum": 0, "default": 3 }
                    },
                    "required": ["name", "vendor", "endpoint", "priority"]
                },
                "CheckRolloutRequest": {
                    "type": "object",
                    "properties": {
                        "canary": { "type": "object" },
                        "user_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["canary", "user_id"]
                },
                "RegisterRolloutRequest": {
                    "type": "object",
                    "properties": {
                        "config": { "type": "object" }
                    },
                    "required": ["config"]
                },
                "FindRolloutInclusionRequest": {
                    "type": "object",
                    "properties": {
                        "rollout_id": { "type": "string" },
                        "feature": { "type": "string" },
                        "user_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["rollout_id", "feature", "user_id"]
                },
                "EvaluateAutoRollbackRequest": {
                    "type": "object",
                    "properties": {
                        "rollout_id": { "type": "string" },
                        "error_rate": { "type": "number", "minimum": 0 },
                        "latency_p99_ms": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["rollout_id", "error_rate", "latency_p99_ms"]
                },
                "AdvanceSegmentRequest": {
                    "type": "object",
                    "properties": {
                        "rollout_id": { "type": "string" },
                        "segment_idx": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["rollout_id", "segment_idx"]
                },
                "DeployWithRollbackRequest": {
                    "type": "object",
                    "properties": {
                        "artifact": { "type": "object" },
                        "rollback_enabled": { "type": "boolean", "default": false }
                    },
                    "required": ["artifact"]
                }
            }
        }
    })
"""##;

    let json = FALLBACK_SPEC_JSON.replace("{VERSION}", env!("CARGO_PKG_VERSION"));
    serde_json::from_str(&json).expect("fallback spec JSON must be valid")
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
