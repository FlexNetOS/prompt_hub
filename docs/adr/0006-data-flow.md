# ADR-0006: Data Flow

## Status
Accepted

## Decision
Prompt lifecycle: Register -> Sanitize -> Store -> Index -> Search -> Bundle.

## Flow
```
User -> CLI/API -> PromptHub -> Sanitizer -> Storage -> FTS5/Embeddings
                                      -> Audit Log (append-only)
                                      -> Sync Events
```
