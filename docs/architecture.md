# Architecture

## C4 Model

### System Context Diagram (C4 Level 1)

```mermaid
C4Context
    title System Context - PromptHub
    Person(user, "Developer / Non-technical User", "Manages and uses LLM prompts")
    System_Boundary(prompt_hub, "PromptHub") {
        System(cli, "prompthub CLI", "Command-line tool for prompt management")
        System(server, "prompthub-server", "HTTP API for programmatic access")
        System(lib, "prompt-hub", "Core library with all business logic")
    }
    System_Ext(llm, "LLM Provider", "OpenAI, Anthropic, etc.")
    System_Ext(db, "libsql/SQLite", "Local database storage")
    
    Rel(user, cli, "Uses for prompt management")
    Rel(user, server, "Uses via HTTP API / SDK")
    Rel(cli, lib, "Calls library functions")
    Rel(server, lib, "Calls library functions")
    Rel(lib, db, "Reads/Writes prompts")
    Rel(lib, llm, "Vibe coding / search embeddings")
```

### Container Diagram (C4 Level 2)

```mermaid
C4Container
    title Container Diagram - PromptHub
    Person(user, "User")
    
    Container_Boundary(cli_app, "CLI Application") {
        Container(cli_binary, "prompthub", "Rust Binary", "clap-based CLI with 36 commands")
        Container(fuzzy_finder, "fuzzy.rs", "Rust", "Fuzzy prompt search")
    }
    
    Container_Boundary(server_app, "HTTP Server") {
        Container(axum_server, "prompthub-server", "Rust + Axum", "REST API with OpenAPI docs")
        Container(middleware, "middleware.rs", "Rust", "CORS, tracing, rate limiting")
        Container(routes, "routes.rs", "Rust", "12 HTTP route handlers")
    }
    
    Container_Boundary(core_lib, "Core Library") {
        Container(hub, "hub.rs", "Rust", "PromptHub engine (18 methods)")
        Container(storage, "storage.rs", "Rust", "libsql persistence layer")
        Container(search, "search.rs", "Rust", "FAST/SMART/Hybrid search")
        Container(auth, "auth.rs", "Rust", "RBAC with argon2id")
        Container(sanitize, "sanitize.rs", "Rust", "5+ injection heuristics")
        Container(vibe, "vibe.rs", "Rust", "Vibe Coding engine")
        Container(evolution, "evolution.rs", "Rust", "Genetic prompt evolution")
    }
    
    ContainerDb(database, "Database", "libsql/SQLite", "Prompts, versions, audit log, embeddings")
    
    Rel(user, cli_binary, "Uses")
    Rel(user, axum_server, "Uses via HTTP")
    Rel(cli_binary, hub, "Calls")
    Rel(axum_server, hub, "Calls")
    Rel(hub, storage, "Uses")
    Rel(hub, search, "Uses")
    Rel(hub, auth, "Uses")
    Rel(storage, database, "SQL queries")
```

## Data Flow

```
[User] → [CLI/HTTP] → [PromptHub::new()] → [Sanitizer] → [Storage]
                                              ↓
                                         [Audit Log]
                                              ↓
                                         [Search Index]
                                              ↓
                                         [Sync Events]
```
