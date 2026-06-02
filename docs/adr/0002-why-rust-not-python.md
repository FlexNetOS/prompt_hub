# ADR-0002: Why Rust over Python

## Status
Accepted

## Context
Prompt management libraries are often written in Python. We chose Rust for PromptHub.

## Decision
Use Rust 2024 Edition (MSRV 1.91.1).

## Rationale
- **Performance**: Zero-cost abstractions, no GIL contention
- **Safety**: No runtime errors from null pointers, data races
- **Embedding**: Can be embedded in other Rust binaries and exposed via C ABI
- **Deployment**: Single static binary, no Python environment needed
- **WASM potential**: (Future) wasm32-wasip2 support in Tokio 1.52+

## Consequences
- Steeper learning curve for contributors
- Smaller ecosystem than Python for ML/NLP
- ONNX Runtime (ort crate) provides access to embedding models
