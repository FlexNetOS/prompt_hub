# ADR-0004: Async Architecture

## Status
Accepted

## Decision
Use Tokio 1.52.3 with native async fn in traits (Rust 2024 Edition).

## Rationale
- No async-trait crate needed (native since 1.75)
- SQLite WAL mode enables concurrent reads during writes
- JoinSet for structured concurrency
- Graceful shutdown via broadcast channels
