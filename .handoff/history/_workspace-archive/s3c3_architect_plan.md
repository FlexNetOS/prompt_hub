# Architect scoping plan — "wire `smart` embedding search end-to-end"

> Produced by `feature-architect` (Plan agent, opus) in prompt-loop session-3 cycle-3 (2026-06-06),
> verified against the code at `origin/main` (8fe0b64). This decomposes the parked multi-cycle epic
> into ordered, single-cycle-sized slices. Slices 1–3 are loop-buildable; 4–5 are blocked pending an
> inference-runtime decision (see Open decisions).

## Findings (verified)
- `SmartEngine::search` embeds the **query only** via `mock_embed` (`search.rs:371`), a deterministic
  `DefaultHasher`-based 384-d vector (`search.rs:336-348`). No real model is ever invoked.
- **No embedding-write path.** `Storage::insert_prompt` (`storage.rs:283-341`) never writes the
  `embeddings` table; `SmartEngine::index`/`HybridEngine::index` are no-ops. So in production the
  `embeddings` table is always empty and SMART returns nothing — only tests manually
  `INSERT INTO embeddings` (`search.rs:1232`, `1293`).
- Stored embeddings read as `F32_BLOB(384)` (col 19), decoded by `bytes_to_f32_vec`
  (`search.rs:410-414`). Schema: `embeddings(prompt_id PK, embedding F32_BLOB(384), created_at)`,
  `ON DELETE CASCADE` (`migrations/0001_initial.sql:64-68`).
- `load_model`/`download_model`/`verify_checksum` (`search.rs:271-309`) are dead stubs.
- The `smart` feature only does `smart = ["dep:ndarray"]` (`Cargo.toml:18`) and **`ndarray` is
  imported nowhere**; `cfg(feature = "smart")` appears nowhere in `src/`. The SMART path actually
  compiles/runs under `default` features — it is NOT behind `smart`.
- `SearchEngine` is object-safe via boxed-future methods (`search.rs:23-47`); `Arc<dyn SearchEngine>`
  compile test at `search.rs:1393`.
- Construction: `PromptHub::new` → `SmartEngine::new(config.embedding_model, storage)` →
  `HybridEngine::new(fast, smart)` (`hub.rs:95,117-119`). Config already carries `embedding_model`
  and `embedding_dimension: 384` (`config.rs:24-25,39-40`).
- `mock_embed` + `cosine_similarity` are public and used by `benches/embedding_generation.rs` —
  blast-radius for any rename.

## Target design
Pluggable, object-safe `Embedder` trait (boxed-future for `dyn`, `Result<_, HubError>`). `SmartEngine`
holds `Arc<dyn Embedder>` instead of calling `self.mock_embed`; the same embedder writes prompt
embeddings on `index`. CI-default backend = deterministic in-memory `HashEmbedder` (today's
`mock_embed` logic relocated), so the full read+write path is testable with no model/network. Real
model backends implement the same trait behind a feature.

```rust
pub trait Embedder: Send + Sync + std::fmt::Debug {
    fn dimension(&self) -> usize;
    fn embed<'a>(&'a self, texts: &'a [String])
        -> Pin<Box<dyn Future<Output = Result<Vec<Vec<f32>>>> + Send + 'a>>;
    fn name(&self) -> &'static str;
}
#[derive(Debug, Clone)]
pub struct HashEmbedder { dim: usize } // embed() == today's mock_embed, generalized over dim
```

## Open decisions (human/runtime — block slices 4–5)
- **Inference runtime:** `ort`/ONNX vs `candle` vs `fastembed` vs remote API. Dep weight, build reqs,
  and `#![forbid(unsafe_code)]` implications differ (some pull `unsafe` FFI).
- **Tokenizer:** reuse the existing `tokenizers` feature for `all-MiniLM-L6-v2`?
- **Model acquisition & CI:** download-at-runtime vs vendored artifact; checksum manifest format;
  is CI allowed network access? Determines whether the real backend can ever be a green PR.
- **Dimension authority:** `config.embedding_dimension` (384) vs hard-coded `F32_BLOB(384)`. A
  different-dimension model needs a new migration.
- **`smart` feature semantics:** should it gate the real backend, and does `ndarray` stay or go?
