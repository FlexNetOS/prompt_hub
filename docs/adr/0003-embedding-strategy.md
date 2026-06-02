# ADR-0003: Embedding Strategy

## Status
Accepted

## Decision
Use ONNX Runtime (fastembed 5.15.0 + ort 2.0.15) with all-MiniLM-L6-v2 (384-dim).

## Rationale
- No separate vector DB needed for <10K prompts (libsql F32_BLOB)
- ONNX runs locally, no API calls needed
- 384-dim is compact yet effective for semantic search
- Optional Qdrant backend for scale-out deployments
