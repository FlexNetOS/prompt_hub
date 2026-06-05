# prompt-loop backlog — prompt_hub construction crew

The **single source of truth** for what the crew builds next. Legend:
`- [ ]` todo · `- [x]` done+verified · `- [!] blocked: <reason>`.
Each item = one cohesive, shippable unit sized to one cycle. Every item cites its source.

> **This is a SEED.** `backlog-curator` rewrites this from real state at DISCOVER (and reconciles
> it each cycle), preserving `[x]`/`[!]` history. The items below are candidate starters derived
> from known deferred work; DISCOVER will confirm, reorder, and expand them against ground truth.

## Core library (prompt-hub)
- [ ] Wire `smart` embedding search end-to-end — replace `mock_embed` with a real embedding
      path behind the `smart` flag (needs an inference-runtime decision + model handling).
      _Source: search.rs SmartEngine/HybridEngine are built but run on a deterministic hash;
      `smart` only gates unused `ndarray`. Likely multi-cycle — architect should scope the smallest
      shippable slice (e.g. pluggable embedder trait + a CI-testable fixture backend)._

## CLI / server (prompthub, prompthub-server)
- [ ] (DISCOVER to populate — e.g. surface metrics summary via CLI, config ergonomics.)

## Docs / infra
- [ ] P4 — drive `cargo doc` warnings to zero (doc-link/missing-docs cleanup).
      _Source: known polish item; verify with `cargo doc --workspace --all-features --no-deps`._
- [ ] P5 — verify the Docker build (`docker/Dockerfile`) and add `.cliff.toml` so the changelog
      generates from Conventional-Commit history. _Source: known polish item; enables docs-scribe's
      automated changelog path._

## Notes / context for the next session
- An `otel` metrics feature was just wired end-to-end and is in review as **PR #28**
  (Prometheus text exposition; dropped the vulnerable `protobuf`/`opentelemetry-prometheus` path).
  DISCOVER should mark related items done if that PR has merged, and check for follow-ups.
- Respect the **Rust-native invariant** (`prompt_hub/CLAUDE.md`): wire features behind their flag
  with tests; treat non-Cargo/foreign guidance as drift to fix, not adopt.
