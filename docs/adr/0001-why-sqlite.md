# ADR-0001: Why SQLite (libsql) over PostgreSQL

## Status
Accepted

## Context
PromptHub needs a database that works in single-user desktop mode, shared team server, CI/CD pipeline, and embedded contexts.

## Decision
Use libsql (Turso's SQLite fork) as the primary database.

## Rationale
- **Zero-config**: Single file, no server process needed
- **Sufficient scale**: Target <10K prompts, SQLite handles this easily
- **Native vector search**: F32_BLOB + DiskANN index built-in
- **Async I/O**: libsql supports async, unlike rusqlite
- **Embedded replicas**: Edge deployment with cloud sync
- **FTS5**: Full-text search built-in
- **WAL mode**: Concurrent readers during writes

## Consequences
- No separate database server needed for most deployments
- For >10K prompts, optional Qdrant backend available
- SQL-compatible: migrations work with standard SQLite tools
