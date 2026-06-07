# prompt-loop backlog — prompt_hub construction crew

The **single source of truth** for what the crew builds next. Legend:
`- [ ]` todo · `- [x]` done+verified · `- [!] blocked: <reason>`.
Each item = one cohesive, shippable unit sized to one cycle. Every item cites its source.

> **Fresh for DISCOVER (2026-06-07):** All previous items resolved across 14 cycles. Run `backlog-curator` to seed new items from TODO.md, docs/audits, feature gaps, and `gh` issues.
> Rust-native invariant applies: wire features behind their flag with tests; foreign/non-Cargo guidance is drift to fix.

---

## ✅ Completed (14 cycles across 6 sessions — all merged)

### SMART_EMBEDDING EPIC (Slices 1-5 deep, PRs #44/#45/#46/#47/#48)
- Extract pluggable Embedder trait + HashEmbedder backend (+7 tests)
- Write prompt embeddings on index via Embedder (storage helpers + integration test)
- Select embedder backend from HubConfig (e2e verified)
- Wire ort-based OrtEmbedder behind smart-ort feature + HubConfig selection
- **Real ONNX inference:** lazy model download, tokenizers, ort::Session, [CLS] extraction, L2-normalize

### Initial setup cycles (PRs #27/#30-#42)
- sha2 0.11 build fix
- Qodana triage (remove 32 unused deps + build fix)
- Prometheus text exposition via otel
- `prompthub metrics` CLI subcommand
- CLI tracing logs → stderr
- RUSTDOCFLAGS=-D warnings in CI
- Docker/Dockerfile verify + .cliff.toml
- CLI local operator identity (RBAC)
- Bench compile fix (criterion::black_box)

### External blockers (not human walls)
- [!] **qodana SARIF regen** — needs `QODANA_TOKEN` + Docker; CI skips without token. Loop proceeds without it.

---

## Pending items (seeded by next DISCOVER)

<empty — backlog cleared for fresh epic>
