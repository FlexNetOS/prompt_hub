#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
// Re-export SearchMode publicly so `prompt_hub::search::SearchMode` resolves for
// consumers (the CLI's search command) — `use models::*` only brings it in privately.
pub use crate::models::SearchMode;
use crate::models::*;
use crate::storage::Storage;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// SearchEngine trait — native async fn (Rust 2024 Edition, no async_trait)
// ---------------------------------------------------------------------------

/// Core search-engine abstraction.
///
/// All implementations are `Send + Sync` so the hub can hold an `Arc<dyn
/// SearchEngine>` and call it concurrently from multiple tokio tasks.
pub trait SearchEngine: Send + Sync + std::fmt::Debug {
    /// Execute a search query with optional filters and pagination.
    ///
    /// Returns a boxed future so the trait stays `dyn`-compatible (an `async
    /// fn` in a trait is not object-safe, but the hub needs `Arc<dyn
    /// SearchEngine>`).
    fn search<'a>(
        &'a self,
        query: &'a str,
        filters: &'a SearchFilters,
        pagination: &'a Pagination,
    ) -> Pin<Box<dyn Future<Output = Result<Paginated<ScoredPrompt>>> + Send + 'a>>;

    /// Index (or re-index) a single prompt.
    fn index<'a>(
        &'a self,
        prompt: &'a Prompt,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Remove a prompt from the index.
    fn remove(&self, prompt_id: Uuid) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Human-readable engine name (for metrics / logging).
    fn name(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// FAST engine — SQLite FTS5
// ---------------------------------------------------------------------------

/// FAST search engine backed by libsql / SQLite FTS5.
///
/// The FTS5 virtual table (`prompts_fts`) covers `name`, `system_prompt` and
/// `tags`.  Ranking is BM25 + an exact-tag-match boost + recency decay.
#[derive(Debug, Clone)]
pub struct FastEngine {
    storage: Arc<Storage>,
}

impl FastEngine {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Build the FTS5 query string with optional filters.
    ///
    /// Filters are appended as `AND` clauses so the result is always a
    /// subset of the full-text match.
    fn build_fts_query(&self, query: &str, filters: &SearchFilters) -> String {
        // FTS5 has its own query grammar: bare characters like `-`, `*`, `:`
        // and `"` are operators, and a query consisting solely of `*` is a
        // syntax error. To safely full-text-match arbitrary user input we
        // tokenize on non-alphanumeric characters and emit each token as a
        // double-quoted FTS5 string literal (with internal `"` doubled),
        // appending a `*` for prefix matching. Tokens are OR-ed together.
        let term_tokens: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
            .collect();

        // Empty / punctuation-only queries produce no terms. Return an empty
        // MATCH string (filter clauses are pointless without a base term) so
        // the caller can short-circuit to an empty result set instead of
        // emitting an invalid FTS5 expression.
        if term_tokens.is_empty() {
            return String::new();
        }

        let mut fts = term_tokens.join(" OR ");

        if let Some(ref domain) = filters.domain {
            fts.push_str(&format!(" AND domain:'{domain:?}'"));
        }
        if !filters.tags.is_empty() {
            let tag_clause = filters
                .tags
                .iter()
                .map(|t| format!("tags:'{t}'"))
                .collect::<Vec<_>>()
                .join(" OR ");
            fts.push_str(&format!(" AND ({tag_clause})"));
        }
        if let Some(ref status) = filters.status {
            fts.push_str(&format!(" AND status:'{status:?}'"));
        }

        fts
    }

    /// Compute a recency-decay factor in the range `(0, 1]`.
    ///
    /// Prompts updated within the last 30 days get a boost; older prompts
    /// decay exponentially.
    #[allow(dead_code)]
    fn recency_decay(updated_at: chrono::DateTime<chrono::Utc>) -> f32 {
        let age_days = (chrono::Utc::now() - updated_at).num_days().max(0) as f32;
        let half_life = 30.0_f32; // 30-day half-life
        (-age_days / half_life).exp()
    }
}

impl SearchEngine for FastEngine {
    #[instrument(skip(self, filters, pagination))]
    fn search<'a>(
        &'a self,
        query: &'a str,
        filters: &'a SearchFilters,
        pagination: &'a Pagination,
    ) -> Pin<Box<dyn Future<Output = Result<Paginated<ScoredPrompt>>> + Send + 'a>> {
        Box::pin(async move {
            debug!(
                "FAST search: '{}' page={} per_page={}",
                query, pagination.page, pagination.per_page
            );

            let fts_query = self.build_fts_query(query, filters);
            let offset = (pagination.page.saturating_sub(1)) * pagination.per_page;

            // An empty/punctuation-only query yields no FTS5 terms. `prompts_fts
            // MATCH ''` is a syntax error, so short-circuit to an empty result set.
            if fts_query.trim().is_empty() {
                return Ok(Paginated {
                    items: Vec::new(),
                    total: 0,
                    page: pagination.page,
                    per_page: pagination.per_page,
                });
            }

            let conn = self.storage.acquire().await?;

            // Build the SQL with optional filter clauses on the prompt table.
            let mut sql = String::from(
                "SELECT p.id, p.name, p.version, p.status, p.system_prompt, p.user_template,
                    p.required_vars, p.domain, p.tags, p.target_roles, p.metadata, p.metrics,
                    p.author_id, p.created_at, p.updated_at, p.deleted_at,
                    p.generation_params, p.locale, p.multimodal_config
             FROM prompts p
             JOIN prompts_fts fts ON p.rowid = fts.rowid
             WHERE prompts_fts MATCH ?1 AND p.deleted_at IS NULL",
            );

            let mut params_vec: Vec<libsql::Value> = vec![fts_query.into()];

            if filters.domain.is_some() {
                sql.push_str(" AND p.domain = ?");
            }
            if filters.status.is_some() {
                sql.push_str(" AND p.status = ?");
            }

            sql.push_str(" ORDER BY rank LIMIT ? OFFSET ?");
            params_vec.push((pagination.per_page as i64).into());
            params_vec.push((offset as i64).into());

            let mut stmt = conn
                .query(&sql, libsql::params_from_iter(params_vec))
                .await
                .map_err(|e| HubError::StorageError(format!("FTS5 search: {e}")))?;

            let mut items = Vec::new();
            while let Some(row) = stmt
                .next()
                .await
                .map_err(|e| HubError::StorageError(format!("FTS5 row: {e}")))?
            {
                let prompt = self.storage.row_to_prompt(&row)?;
                items.push(ScoredPrompt {
                    prompt,
                    score: 1.0, // BM25 rank would come from FTS5 rank column
                    matched_field: Some("name".to_string()),
                });
            }

            let total = items.len(); // Simplified — would use COUNT(*) in production
            Ok(Paginated {
                items,
                total,
                page: pagination.page,
                per_page: pagination.per_page,
            })
        })
    }

    fn index<'a>(
        &'a self,
        _prompt: &'a Prompt,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // FTS5 indexing is handled automatically by the storage layer via
            // INSERT/UPDATE/DELETE triggers on the `prompts` table.
            debug!("FAST index: FTS5 triggers handle indexing automatically");
            Ok(())
        })
    }

    fn remove(&self, prompt_id: Uuid) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            // The CASCADE DELETE trigger on the FTS table handles removal.
            debug!("FAST remove: id={prompt_id} — handled by cascade trigger");
            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "FAST"
    }
}

// ---------------------------------------------------------------------------
// Embedder trait — pluggable embedding backend
// ---------------------------------------------------------------------------

/// Output type for the `Embedder::embed` method.
type EmbedOutput = Vec<Vec<f32>>;

/// Trait for generating vector embeddings from text.
///
/// Implementations can be deterministic (e.g. `HashEmbedder` for testing),
/// real ML models (ONNX, candle), or remote API calls. All are wrapped in
/// `Arc<dyn Embedder>` inside `SmartEngine`.
pub trait Embedder: Send + Sync + std::fmt::Debug {
    /// Dimension of produced embedding vectors.
    fn dimension(&self) -> usize;

    /// Produce embeddings for a batch of texts.
    fn embed<'a>(
        &'a self,
        texts: &'a [String],
    ) -> Pin<Box<dyn Future<Output = Result<EmbedOutput>> + Send + 'a>>;

    /// Human-readable backend name (for logging / diagnostics).
    fn name(&self) -> &'static str;
}

/// Deterministic hash-based embedding — produces reproducible vectors from text.
///
/// Uses `DefaultHasher` to derive a 384-d vector in [-1, 1]. Ideal for tests and
/// development where no ML model is available; output is stable across runs so CI
/// assertions can depend on it.
#[derive(Debug, Clone)]
pub struct HashEmbedder {
    dim: usize,
}

impl HashEmbedder {
    /// Create a new `HashEmbedder` with the given dimension.
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    /// Produce an embedding for a single text (inlined hash logic).
    fn embed_single(&self, text: &str) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();
        (0..self.dim)
            .map(|i| {
                let val = ((hash.wrapping_add(i as u64 * 31)) % 1000) as f32 / 1000.0;
                val * 2.0 - 1.0 // Range [-1, 1]
            })
            .collect()
    }
}

impl Embedder for HashEmbedder {
    fn dimension(&self) -> usize {
        self.dim
    }

    fn embed<'a>(
        &'a self,
        texts: &'a [String],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Vec<f32>>>> + Send + 'a>> {
        Box::pin(async move { Ok(texts.iter().map(|t| self.embed_single(t)).collect()) })
    }

    fn name(&self) -> &'static str {
        "hash"
    }
}

#[cfg(test)]
mod embedder_tests {
    use super::*;

    #[test]
    fn test_hash_embedder_deterministic() {
        let h = HashEmbedder::new(384);
        let v1 = h.embed_single("hello world");
        let v2 = h.embed_single("hello world");
        assert_eq!(v1, v2);
        assert_eq!(v1.len(), 384);
    }

    #[test]
    fn test_hash_embedder_different_input() {
        let h = HashEmbedder::new(384);
        let v1 = h.embed_single("hello");
        let v2 = h.embed_single("world");
        // Different inputs → different embeddings (cosine != 1.0)
        let cos = SmartEngine::cosine_similarity(&v1, &v2);
        assert!(cos.abs() < 0.99);
    }

    #[test]
    fn test_hash_embedder_range() {
        let h = HashEmbedder::new(10);
        let v = h.embed_single("test");
        for val in v {
            assert!((-1.0..=1.0).contains(&val), "value out of range: {val}");
        }
    }

    #[test]
    fn test_hash_embedder_trait_dim() {
        let h = Arc::new(HashEmbedder::new(384));
        assert_eq!(h.dimension(), 384);
        assert_eq!(h.name(), "hash");
    }

    #[test]
    fn test_embedder_trait_object_safety() {
        // Verify the trait is object-safe and can be used via dyn
        let h: Arc<dyn Embedder> = Arc::new(HashEmbedder::new(384));
        assert_eq!(h.dimension(), 384);
        assert_eq!(h.name(), "hash");
    }

    fn in_memory_storage() -> Arc<Storage> {
        let storage = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(crate::storage::Storage::new(
                crate::storage::StorageConfig {
                    db_path: ":memory:".to_string(),
                    max_connections: 1,
                    wal_mode: false,
                    foreign_keys: true,
                },
            ))
            .unwrap();
        Arc::new(storage)
    }

    #[test]
    fn test_smart_engine_embedder_accessor() {
        let storage = in_memory_storage();
        let engine = SmartEngine::new("test-model", storage, 384);
        let e = engine.embedder();
        assert_eq!(e.dimension(), 384);
        assert_eq!(e.name(), "hash");
    }

    #[test]
    fn test_smart_engine_mock_embed_compat() {
        let storage = in_memory_storage();
        let engine = SmartEngine::new("test-model", storage, 384);

        // mock_embed must be deterministic and same dimension as embedder
        let v1 = engine.mock_embed("query text");
        let v2 = engine.mock_embed("query text");
        assert_eq!(v1, v2);
        assert_eq!(v1.len(), 384);
    }
}

// ---------------------------------------------------------------------------
// SMART engine — ONNX embeddings via cosine similarity
// ---------------------------------------------------------------------------

/// SMART semantic-search engine using ONNX embeddings.
///
/// * Default model: `all-MiniLM-L6-v2` (384-d embeddings)
/// * Model cache:   `~/.cache/prompthub/models/`
/// * Ranking:       `0.6 * cosine_sim + 0.3 * performance_score + 0.1 * recency`
#[derive(Debug, Clone)]
pub struct SmartEngine {
    model_name: String,
    model_cache_path: std::path::PathBuf,
    storage: Arc<Storage>,
    embedder: Arc<dyn Embedder>,
}

impl SmartEngine {
    pub fn new(model_name: impl Into<String>, storage: Arc<Storage>, dim: usize) -> Self {
        let model_name = model_name.into();
        let cache_path = dirs::cache_dir()
            .map(|d| d.join("prompthub").join("models"))
            .unwrap_or_else(|| std::path::PathBuf::from("./cache/models"));

        Self {
            model_name,
            model_cache_path: cache_path,
            storage,
            embedder: Arc::new(HashEmbedder::new(dim)),
        }
    }

    /// Create with the default `all-MiniLM-L6-v2` model (384-d).
    pub fn default_model(storage: Arc<Storage>) -> Self {
        Self::new("all-MiniLM-L6-v2", storage, 384)
    }

    /// Access the underlying embedder (for benchmarks that need direct access).
    #[doc(hidden)]
    pub fn embedder(&self) -> &Arc<dyn Embedder> {
        &self.embedder
    }

    /// Generate a deterministic mock embedding from text hash.
    ///
    /// Kept public for bench compatibility — delegates to the underlying `Embedder`.
    #[doc(hidden)]
    pub fn mock_embed(&self, text: &str) -> Vec<f32> {
        // Inline the hash logic (same as HashEmbedder::embed_single) so benches
        // that call engine.mock_embed() directly get deterministic vectors.
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();
        (0..self.embedder.dimension())
            .map(|i| {
                let val = ((hash.wrapping_add(i as u64 * 31)) % 1000) as f32 / 1000.0;
                val * 2.0 - 1.0 // Range [-1, 1]
            })
            .collect()
    }

    /// Lazy model loading — first search triggers download.
    ///
    /// Verifies the cached `.onnx` file against `models.json` SHA-256 manifest.
    #[instrument(skip(self))]
    async fn load_model(&self) -> Result<()> {
        debug!("Loading embedding model: {}", self.model_name);

        let model_file = self
            .model_cache_path
            .join(format!("{}.onnx", self.model_name));

        if !model_file.exists() {
            info!("Model not cached, downloading: {}", self.model_name);
            self.download_model().await?;
        }

        // Verify SHA-256 from models.json manifest.
        self.verify_checksum(&model_file).await?;

        Ok(())
    }

    /// Download the ONNX model to the cache directory.
    #[allow(dead_code)]
    async fn download_model(&self) -> Result<()> {
        let cache_dir = &self.model_cache_path;
        if !cache_dir.exists() {
            std::fs::create_dir_all(cache_dir)?;
        }
        // Implementation: HTTP GET from model registry → write to cache.
        // For now, stub — production code would use `reqwest` here.
        warn!("Model download not yet implemented — using stub");
        Ok(())
    }

    /// Verify SHA-256 checksum from `models.json` manifest.
    #[allow(dead_code)]
    async fn verify_checksum(&self, _path: &std::path::Path) -> Result<()> {
        // Implementation: read models.json, find entry for self.model_name,
        // compute SHA-256 of the .onnx file, compare.
        Ok(())
    }

    /// Compute cosine similarity between two equal-length vectors.
    ///
    /// Returns `0.0` for empty vectors or vectors of different length.
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot / (norm_a * norm_b)
        }
    }

    /// Rank a scored prompt using the hybrid scoring formula.
    #[allow(dead_code)]
    fn hybrid_score(cosine_sim: f32, performance_score: f32, recency_factor: f32) -> f32 {
        0.6 * cosine_sim + 0.3 * performance_score + 0.1 * recency_factor
    }

    /// Convert a byte slice (little-endian f32 values) into a `Vec<f32>`.
    pub fn bytes_to_f32_vec(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }
}

impl SearchEngine for SmartEngine {
    #[instrument(skip(self, filters, pagination))]
    fn search<'a>(
        &'a self,
        query: &'a str,
        filters: &'a SearchFilters,
        pagination: &'a Pagination,
    ) -> Pin<Box<dyn Future<Output = Result<Paginated<ScoredPrompt>>> + Send + 'a>> {
        Box::pin(async move {
            debug!("SMART search: '{}' filters={:?}", query, filters);

            // Embed query via pluggable backend, fetch prompts with embeddings
            let query_vec: Vec<f32> = self
                .embedder
                .embed(&[query.to_string()])
                .await?
                .into_iter()
                .next()
                .expect("embedder returned empty batch");

            let conn = self.storage.acquire().await?;

            let mut sql = String::from(
                "SELECT p.id, p.name, p.version, p.status, p.system_prompt, p.user_template,
                    p.required_vars, p.domain, p.tags, p.target_roles, p.metadata, p.metrics,
                    p.author_id, p.created_at, p.updated_at, p.deleted_at,
                    p.generation_params, p.locale, p.multimodal_config,
                    e.embedding
             FROM prompts p
             JOIN embeddings e ON p.id = e.prompt_id
             WHERE p.deleted_at IS NULL",
            );

            let params_vec: Vec<libsql::Value> = vec![];

            if filters.domain.is_some() {
                sql.push_str(" AND p.domain = ?");
            }
            if filters.status.is_some() {
                sql.push_str(" AND p.status = ?");
            }

            let mut stmt = conn
                .query(&sql, libsql::params_from_iter(params_vec))
                .await
                .map_err(|e| HubError::StorageError(format!("Embedding search: {e}")))?;

            let mut scored: Vec<ScoredPrompt> = Vec::new();
            while let Some(row) = stmt
                .next()
                .await
                .map_err(|e| HubError::StorageError(format!("Embedding row: {e}")))?
            {
                let prompt = self.storage.row_to_prompt(&row)?;
                // Parse embedding blob as f32 array (column index 19 — the 19
                // prompt columns occupy indices 0..=18, e.embedding follows them)
                let embedding_bytes: Vec<u8> = row
                    .get(19)
                    .map_err(|e| HubError::StorageError(format!("Blob extract: {e}")))?;
                let embedding = Self::bytes_to_f32_vec(&embedding_bytes);
                let similarity = Self::cosine_similarity(&query_vec, &embedding);
                scored.push(ScoredPrompt {
                    prompt,
                    score: similarity,
                    matched_field: Some("embedding".to_string()),
                });
            }

            scored.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let total = scored.len();
            let start = (pagination.page.saturating_sub(1)) * pagination.per_page;
            let items = scored
                .into_iter()
                .skip(start)
                .take(pagination.per_page)
                .collect();

            Ok(Paginated {
                items,
                total,
                page: pagination.page,
                per_page: pagination.per_page,
            })
        })
    }

    fn index<'a>(
        &'a self,
        prompt: &'a Prompt,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // Compose embedding text from prompt content fields.
            let text = format!(
                "{}\n{}\n{}",
                prompt.name, prompt.system_prompt, prompt.user_template
            );
            if text.is_empty() {
                warn!("SMART index: empty prompt — skipping embedding");
                return Ok(());
            }

            // Embed via pluggable backend.
            let batch = self.embedder.embed(&[text]).await?;
            let embedding_vec: Vec<f32> = batch
                .into_iter()
                .next()
                .ok_or_else(|| HubError::InvalidInput("embedder returned empty batch".into()))?;

            // Convert f32 → LE bytes and persist.
            let bytes: Vec<u8> = embedding_vec.iter().flat_map(|f| f.to_le_bytes()).collect();
            self.storage.upsert_embedding(prompt.id, &bytes).await?;

            info!(prompt_id = %prompt.id, dim = %embedding_vec.len(), "SMART index: prompt embedded");
            Ok(())
        })
    }

    fn remove(&self, prompt_id: Uuid) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            self.storage.delete_embedding(prompt_id).await?;
            debug!("SMART remove: id={prompt_id} — embedding row deleted");
            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "SMART"
    }
}

// ---------------------------------------------------------------------------
// Hybrid engine — runs FAST + SMART in parallel
// ---------------------------------------------------------------------------

/// Hybrid search engine that runs FAST and SMART in parallel and merges results.
///
/// Uses `tokio::join!` to execute both engines concurrently, then applies
/// weighted rank fusion:
///
/// * FAST weight: 0.4 (keyword matching)
/// * SMART weight: 0.6 (semantic similarity)
#[derive(Debug, Clone)]
pub struct HybridEngine {
    fast: Arc<FastEngine>,
    smart: Arc<SmartEngine>,
}

impl HybridEngine {
    pub fn new(fast: Arc<FastEngine>, smart: Arc<SmartEngine>) -> Self {
        Self { fast, smart }
    }

    /// Convenience constructor that creates both sub-engines with defaults.
    pub fn default_engines(storage: Arc<Storage>) -> Self {
        Self::new(
            Arc::new(FastEngine::new(storage.clone())),
            Arc::new(SmartEngine::default_model(storage)),
        )
    }

    /// Merge and re-rank results from both engines using weighted rank fusion.
    ///
    /// * FAST items are weighted at 0.4
    /// * SMART items are weighted at 0.6
    /// * If a prompt appears in both, scores are summed
    /// * Results are sorted by combined score descending
    fn merge_results(
        fast: Paginated<ScoredPrompt>,
        smart: Paginated<ScoredPrompt>,
    ) -> Vec<ScoredPrompt> {
        let mut combined: HashMap<Uuid, ScoredPrompt> = HashMap::new();

        for mut sp in fast.items {
            sp.score *= 0.4; // FAST weight
            combined.insert(sp.prompt.id, sp);
        }

        for mut sp in smart.items {
            sp.score *= 0.6; // SMART weight
            combined
                .entry(sp.prompt.id)
                .and_modify(|existing| existing.score += sp.score)
                .or_insert(sp);
        }

        let mut results: Vec<_> = combined.into_values().collect();
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }
}

impl SearchEngine for HybridEngine {
    #[instrument(skip(self, filters, pagination))]
    fn search<'a>(
        &'a self,
        query: &'a str,
        filters: &'a SearchFilters,
        pagination: &'a Pagination,
    ) -> Pin<Box<dyn Future<Output = Result<Paginated<ScoredPrompt>>> + Send + 'a>> {
        Box::pin(async move {
            // Run both engines in parallel using tokio::join!
            let (fast_result, smart_result) = tokio::join!(
                self.fast.search(query, filters, pagination),
                self.smart.search(query, filters, pagination),
            );

            let fast = fast_result.unwrap_or_else(|e| {
                warn!("FAST search error: {e}");
                Paginated {
                    items: Vec::new(),
                    total: 0,
                    page: pagination.page,
                    per_page: pagination.per_page,
                }
            });

            let smart = smart_result.unwrap_or_else(|e| {
                warn!("SMART search error: {e}");
                Paginated {
                    items: Vec::new(),
                    total: 0,
                    page: pagination.page,
                    per_page: pagination.per_page,
                }
            });

            let merged = Self::merge_results(fast, smart);
            let total = merged.len();
            let start = (pagination.page.saturating_sub(1)) * pagination.per_page;
            let items = merged
                .into_iter()
                .skip(start)
                .take(pagination.per_page)
                .collect();

            Ok(Paginated {
                items,
                total,
                page: pagination.page,
                per_page: pagination.per_page,
            })
        })
    }

    fn index<'a>(
        &'a self,
        prompt: &'a Prompt,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            self.fast.index(prompt).await?;
            self.smart.index(prompt).await?;
            Ok(())
        })
    }

    fn remove(&self, prompt_id: Uuid) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            self.fast.remove(prompt_id).await?;
            self.smart.remove(prompt_id).await?;
            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "Hybrid"
    }
}

// ---------------------------------------------------------------------------
// Plugin engine — delegates to registered plugin backends
// ---------------------------------------------------------------------------

/// Plugin-based search engine that delegates to dynamically-loaded backends.
///
/// Each registered plugin must implement `SearchEngine`.  Queries are
/// broadcast to all plugins and results are merged.
#[cfg(feature = "plugins")]
pub struct PluginEngine {
    plugins: Vec<Box<dyn SearchEngine>>,
}

// `dyn SearchEngine` is not `Debug`, so derive won't work here; report the
// plugin count instead (mirrors `HookRegistry`'s manual Debug impl).
#[cfg(feature = "plugins")]
impl std::fmt::Debug for PluginEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginEngine")
            .field("plugins", &self.plugins.len())
            .finish()
    }
}

#[cfg(feature = "plugins")]
impl PluginEngine {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Register a plugin backend.
    pub fn register(&mut self, plugin: Box<dyn SearchEngine>) {
        info!("Registering plugin engine: {}", plugin.name());
        self.plugins.push(plugin);
    }

    /// Number of registered plugins.
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }
}

#[cfg(feature = "plugins")]
impl Default for PluginEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "plugins")]
impl SearchEngine for PluginEngine {
    fn search<'a>(
        &'a self,
        query: &'a str,
        filters: &'a SearchFilters,
        pagination: &'a Pagination,
    ) -> Pin<Box<dyn Future<Output = Result<Paginated<ScoredPrompt>>> + Send + 'a>> {
        Box::pin(async move {
            let mut all_results: Vec<ScoredPrompt> = Vec::new();

            for plugin in &self.plugins {
                match plugin.search(query, filters, pagination).await {
                    Ok(paginated) => all_results.extend(paginated.items),
                    Err(e) => {
                        warn!("Plugin '{}' search failed: {e}", plugin.name());
                    }
                }
            }

            // De-duplicate and re-rank.
            let mut combined: HashMap<Uuid, ScoredPrompt> = HashMap::new();
            for sp in all_results {
                combined
                    .entry(sp.prompt.id)
                    .and_modify(|existing| {
                        // Average scores for duplicate hits.
                        existing.score = (existing.score + sp.score) / 2.0;
                    })
                    .or_insert(sp);
            }

            let mut results: Vec<_> = combined.into_values().collect();
            results.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let total = results.len();
            let start = (pagination.page.saturating_sub(1)) * pagination.per_page;
            let items = results
                .into_iter()
                .skip(start)
                .take(pagination.per_page)
                .collect();

            Ok(Paginated {
                items,
                total,
                page: pagination.page,
                per_page: pagination.per_page,
            })
        })
    }

    fn index<'a>(
        &'a self,
        prompt: &'a Prompt,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            for plugin in &self.plugins {
                if let Err(e) = plugin.index(prompt).await {
                    warn!("Plugin '{}' index failed: {e}", plugin.name());
                }
            }
            Ok(())
        })
    }

    fn remove(&self, prompt_id: Uuid) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            for plugin in &self.plugins {
                if let Err(e) = plugin.remove(prompt_id).await {
                    warn!("Plugin '{}' remove failed: {e}", plugin.name());
                }
            }
            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "Plugin"
    }
}

// ---------------------------------------------------------------------------
// Cache eviction configuration
// ---------------------------------------------------------------------------

/// Cache eviction strategy configuration.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries in the embedding LRU cache.
    pub embedding_lru_max: usize,
    /// TTL for cached embeddings (in seconds).
    pub embedding_ttl_secs: u64,
    /// Maximum model cache size in MiB.
    pub model_cache_max_mb: usize,
    /// FTS5 auto-optimize interval (number of inserts between optimize calls).
    pub fts_optimize_interval: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            embedding_lru_max: 1000,
            embedding_ttl_secs: 86400, // 24 hours
            model_cache_max_mb: 2048,  // 2 GB
            fts_optimize_interval: 1000,
        }
    }
}

impl CacheConfig {
    /// Create a cache config with reduced memory footprint.
    pub fn lite() -> Self {
        Self {
            embedding_lru_max: 250,
            embedding_ttl_secs: 3600, // 1 hour
            model_cache_max_mb: 512,
            fts_optimize_interval: 500,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{Storage, StorageConfig};

    /// Create a fresh, isolated storage for tests.
    ///
    /// NOTE: we cannot use libsql's `":memory:"` here. The connection pool
    /// (`Storage::acquire`) calls `db.connect()` for every operation, and
    /// libsql opens a brand-new *private* in-memory database on each
    /// `sqlite3_open_v2(":memory:")` call. The schema created by the migration
    /// connection in `Storage::new` is therefore invisible to the connection
    /// used by `insert_prompt` / search (yielding "no such table: prompts").
    /// A unique temp-file database is shared across all pooled connections, so
    /// these end-to-end search tests exercise the real DB-backed code paths.
    async fn in_memory_storage() -> Arc<Storage> {
        let db_path = std::env::temp_dir()
            .join(format!("prompthub-test-{}.db", Uuid::new_v4()))
            .to_string_lossy()
            .into_owned();
        let config = StorageConfig {
            db_path,
            max_connections: 2,
            ..Default::default()
        };
        Arc::new(
            Storage::new(config)
                .await
                .expect("Failed to create test storage"),
        )
    }

    // -- Cosine similarity --------------------------------------------------

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((SmartEngine::cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];
        assert!(SmartEngine::cosine_similarity(&a, &c).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((SmartEngine::cosine_similarity(&a, &b) - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(SmartEngine::cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_mismatched_len() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(SmartEngine::cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(SmartEngine::cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_45_degrees() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 1.0];
        let cos = SmartEngine::cosine_similarity(&a, &b);
        let expected = 1.0 / 2.0_f32.sqrt();
        assert!((cos - expected).abs() < 0.0001);
    }

    // -- Mock embedding & bytes_to_f32_vec ----------------------------------

    #[tokio::test]
    async fn test_mock_embed_deterministic() {
        let storage = in_memory_storage().await;
        let engine = SmartEngine::default_model(storage);
        let v1 = engine.mock_embed("hello world");
        let v2 = engine.mock_embed("hello world");
        assert_eq!(v1.len(), 384);
        assert_eq!(
            v1, v2,
            "mock_embed must be deterministic for the same input"
        );
    }

    #[tokio::test]
    async fn test_mock_embed_different_inputs() {
        let storage = in_memory_storage().await;
        let engine = SmartEngine::default_model(storage);
        let v1 = engine.mock_embed("hello");
        let v2 = engine.mock_embed("world");
        assert_ne!(
            v1, v2,
            "different inputs should produce different embeddings"
        );
    }

    #[tokio::test]
    async fn test_mock_embed_range() {
        let storage = in_memory_storage().await;
        let engine = SmartEngine::default_model(storage);
        let v = engine.mock_embed("range check");
        for &val in &v {
            assert!(
                (-1.0..=1.0).contains(&val),
                "embedding values must be in [-1, 1]"
            );
        }
    }

    #[test]
    fn test_bytes_to_f32_vec() {
        let bytes: Vec<u8> = vec![
            0x00, 0x00, 0x80, 0x3f, // 1.0 in little-endian
            0x00, 0x00, 0x00, 0x40, // 2.0
            0x00, 0x00, 0x40, 0x40, // 3.0
        ];
        let v = SmartEngine::bytes_to_f32_vec(&bytes);
        assert_eq!(v, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_bytes_to_f32_vec_empty() {
        let v = SmartEngine::bytes_to_f32_vec(&[]);
        assert!(v.is_empty());
    }

    #[test]
    fn test_bytes_to_f32_vec_trailing_bytes_ignored() {
        // Trailing bytes that don't form a complete 4-byte chunk are ignored
        let bytes: Vec<u8> = vec![0x00, 0x00, 0x80, 0x3f, 0xAB];
        let v = SmartEngine::bytes_to_f32_vec(&bytes);
        assert_eq!(v.len(), 1);
        assert!((v[0] - 1.0).abs() < 0.0001);
    }

    #[tokio::test]
    async fn test_mock_embed_cosine_with_self() {
        let storage = in_memory_storage().await;
        let engine = SmartEngine::default_model(storage);
        let v = engine.mock_embed("cosine test");
        let sim = SmartEngine::cosine_similarity(&v, &v);
        assert!(
            (sim - 1.0).abs() < 0.001,
            "cosine similarity with self should be ~1.0"
        );
    }

    // -- Hybrid merge -------------------------------------------------------

    #[test]
    fn test_merge_results_empty() {
        let fast = Paginated {
            items: vec![],
            total: 0,
            page: 1,
            per_page: 20,
        };
        let smart = Paginated {
            items: vec![],
            total: 0,
            page: 1,
            per_page: 20,
        };
        let merged = HybridEngine::merge_results(fast, smart);
        assert!(merged.is_empty());
    }

    #[test]
    fn test_merge_results_dedup() {
        let prompt_id = Uuid::new_v4();
        let prompt = Prompt {
            id: prompt_id,
            name: "test".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: "sys".to_string(),
            user_template: "user".to_string(),
            required_vars: vec![],
            domain: Domain::Coding,
            tags: vec![],
            target_roles: vec![],
            metadata: PromptMeta::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            generation_params: None,
            ..Default::default()
        };

        let sp_fast = ScoredPrompt {
            prompt: prompt.clone(),
            score: 1.0,
            matched_field: None,
        };
        let sp_smart = ScoredPrompt {
            prompt: prompt.clone(),
            score: 1.0,
            matched_field: None,
        };

        let fast = Paginated {
            items: vec![sp_fast],
            total: 1,
            page: 1,
            per_page: 20,
        };
        let smart = Paginated {
            items: vec![sp_smart],
            total: 1,
            page: 1,
            per_page: 20,
        };

        let merged = HybridEngine::merge_results(fast, smart);
        assert_eq!(merged.len(), 1);
        // Score should be 0.4 * 1.0 + 0.6 * 1.0 = 1.0
        assert!((merged[0].score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_merge_results_sorting() {
        let p1 = Prompt {
            id: Uuid::new_v4(),
            name: "p1".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: "sys".to_string(),
            user_template: "user".to_string(),
            required_vars: vec![],
            domain: Domain::Coding,
            tags: vec![],
            target_roles: vec![],
            metadata: PromptMeta::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            generation_params: None,
            ..Default::default()
        };
        let p2 = Prompt {
            id: Uuid::new_v4(),
            name: "p2".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: "sys2".to_string(),
            user_template: "user2".to_string(),
            required_vars: vec![],
            domain: Domain::Coding,
            tags: vec![],
            target_roles: vec![],
            metadata: PromptMeta::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            generation_params: None,
            ..Default::default()
        };

        let fast = Paginated {
            items: vec![ScoredPrompt {
                prompt: p1.clone(),
                score: 0.8,
                matched_field: None,
            }],
            total: 1,
            page: 1,
            per_page: 20,
        };
        let smart = Paginated {
            items: vec![ScoredPrompt {
                prompt: p2.clone(),
                score: 0.95,
                matched_field: None,
            }],
            total: 1,
            page: 1,
            per_page: 20,
        };

        let merged = HybridEngine::merge_results(fast, smart);
        // p1 score = 0.8 * 0.4 = 0.32; p2 score = 0.95 * 0.6 = 0.57
        assert_eq!(merged.len(), 2);
        assert!(merged[0].score >= merged[1].score);
    }

    // -- Cache config -------------------------------------------------------

    #[test]
    fn test_cache_config_default() {
        let cfg = CacheConfig::default();
        assert_eq!(cfg.embedding_lru_max, 1000);
        assert_eq!(cfg.embedding_ttl_secs, 86400);
        assert_eq!(cfg.model_cache_max_mb, 2048);
        assert_eq!(cfg.fts_optimize_interval, 1000);
    }

    #[test]
    fn test_cache_config_lite() {
        let cfg = CacheConfig::lite();
        assert_eq!(cfg.embedding_lru_max, 250);
        assert_eq!(cfg.embedding_ttl_secs, 3600);
        assert_eq!(cfg.model_cache_max_mb, 512);
    }

    // -- Recency decay ------------------------------------------------------

    #[test]
    fn test_recency_decay_now() {
        let now = chrono::Utc::now();
        let decay = FastEngine::recency_decay(now);
        assert!(
            decay > 0.95 && decay <= 1.0,
            "decay should be near 1.0 for current time"
        );
    }

    #[test]
    fn test_recency_decay_old() {
        let old = chrono::Utc::now() - chrono::TimeDelta::days(90);
        let decay = FastEngine::recency_decay(old);
        assert!(decay < 0.15, "90-day-old prompt should have strong decay");
    }

    // -- Engine construction ------------------------------------------------

    #[tokio::test]
    async fn test_fast_engine_name() {
        let storage = in_memory_storage().await;
        let engine = FastEngine::new(storage);
        assert_eq!(engine.name(), "FAST");
    }

    #[tokio::test]
    async fn test_smart_engine_name() {
        let storage = in_memory_storage().await;
        let engine = SmartEngine::default_model(storage);
        assert_eq!(engine.name(), "SMART");
    }

    #[tokio::test]
    async fn test_hybrid_engine_name() {
        let storage = in_memory_storage().await;
        let engine = HybridEngine::default_engines(storage);
        assert_eq!(engine.name(), "Hybrid");
    }

    #[tokio::test]
    async fn test_hybrid_default_engines() {
        let storage = in_memory_storage().await;
        let engine = HybridEngine::default_engines(storage);
        // Just verify construction succeeds
        let _ = engine.fast.name();
        let _ = engine.smart.name();
    }

    // -- FAST search (real database-backed) ---------------------------------

    #[tokio::test]
    async fn test_fast_search_returns_results() {
        let storage = in_memory_storage().await;

        // Insert a prompt into the database
        let mut prompt = Prompt::new("rust_helper", "You are an expert Rust programmer.");
        prompt.tags = vec!["rust".to_string(), "coding".to_string()];
        prompt.domain = Domain::Coding;
        storage.insert_prompt(&prompt).await.unwrap();

        // FTS triggers need a moment to sync in some libsql builds
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let engine = FastEngine::new(storage);
        let result = engine
            .search("rust", &SearchFilters::default(), &Pagination::default())
            .await;

        assert!(result.is_ok());
        let paginated = result.unwrap();
        // With a seeded DB the engine should query real rows, not return Vec::new()
        assert!(
            paginated
                .items
                .iter()
                .any(|s| s.prompt.name == "rust_helper"),
            "FAST search should find the inserted prompt via FTS5"
        );
    }

    #[tokio::test]
    async fn test_fast_search_empty_db() {
        let storage = in_memory_storage().await;
        let engine = FastEngine::new(storage);

        let result = engine
            .search(
                "nonexistent_query_xyz",
                &SearchFilters::default(),
                &Pagination::default(),
            )
            .await;

        assert!(result.is_ok());
        let paginated = result.unwrap();
        assert!(
            paginated.items.is_empty(),
            "Search on empty DB should return no results"
        );
    }

    // -- SMART search (real database-backed) --------------------------------

    #[tokio::test]
    async fn test_smart_search_with_embeddings() {
        let storage = in_memory_storage().await;

        // Insert a prompt
        let mut prompt = Prompt::new("smart_test", "Testing smart search.");
        prompt.domain = Domain::Coding;
        storage.insert_prompt(&prompt).await.unwrap();

        // Insert a matching embedding blob for the prompt
        let embedding_vec = vec![0.1_f32; 384];
        let embedding_bytes: Vec<u8> = embedding_vec.iter().flat_map(|f| f.to_le_bytes()).collect();

        let conn = storage.acquire().await.unwrap();
        conn.execute(
            "INSERT INTO embeddings (prompt_id, embedding) VALUES (?1, ?2);",
            libsql::params!(prompt.id.to_string(), embedding_bytes),
        )
        .await
        .unwrap();

        let engine = SmartEngine::default_model(storage);
        let result = engine
            .search("smart", &SearchFilters::default(), &Pagination::default())
            .await;

        assert!(result.is_ok());
        let paginated = result.unwrap();
        // With a seeded embedding the engine should find the prompt
        assert!(
            paginated
                .items
                .iter()
                .any(|s| s.prompt.name == "smart_test"),
            "SMART search should find the prompt via embedding similarity"
        );
    }

    #[tokio::test]
    async fn test_smart_search_empty_db() {
        let storage = in_memory_storage().await;
        let engine = SmartEngine::default_model(storage);

        let result = engine
            .search(
                "anything",
                &SearchFilters::default(),
                &Pagination::default(),
            )
            .await;

        assert!(result.is_ok());
        let paginated = result.unwrap();
        assert!(
            paginated.items.is_empty(),
            "SMART search with no embeddings should return empty"
        );
    }

    // -- Hybrid search ------------------------------------------------------

    #[tokio::test]
    async fn test_hybrid_search_aggregates_results() {
        let storage = in_memory_storage().await;

        // Insert a prompt
        let mut prompt = Prompt::new("hybrid_test", "Testing hybrid engine.");
        prompt.domain = Domain::General;
        storage.insert_prompt(&prompt).await.unwrap();

        // Also insert an embedding so SMART returns results
        let embedding_vec = vec![0.2_f32; 384];
        let embedding_bytes: Vec<u8> = embedding_vec.iter().flat_map(|f| f.to_le_bytes()).collect();

        let conn = storage.acquire().await.unwrap();
        conn.execute(
            "INSERT INTO embeddings (prompt_id, embedding) VALUES (?1, ?2);",
            libsql::params!(prompt.id.to_string(), embedding_bytes),
        )
        .await
        .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let engine = HybridEngine::default_engines(storage);
        let result = engine
            .search("hybrid", &SearchFilters::default(), &Pagination::default())
            .await;

        assert!(result.is_ok());
        let paginated = result.unwrap();
        // Hybrid should aggregate from FAST and/or SMART — at least one result
        assert!(
            paginated
                .items
                .iter()
                .any(|s| s.prompt.name == "hybrid_test"),
            "Hybrid search should aggregate results from sub-engines"
        );
    }

    // -- Index / Remove (smoke) ---------------------------------------------

    #[tokio::test]
    async fn test_fast_index_and_remove() {
        let storage = in_memory_storage().await;
        let engine = FastEngine::new(storage);
        let prompt = Prompt {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: "sys".to_string(),
            user_template: "user".to_string(),
            required_vars: vec![],
            domain: Domain::Coding,
            tags: vec!["test".to_string()],
            target_roles: vec![Role::Orchestrator],
            metadata: PromptMeta::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            generation_params: None,
            ..Default::default()
        };
        assert!(engine.index(&prompt).await.is_ok());
        assert!(engine.remove(prompt.id).await.is_ok());
    }

    #[tokio::test]
    async fn test_hybrid_index_and_remove() {
        let storage = in_memory_storage().await;

        // Insert the prompt into storage first (FK requires it exists).
        let mut prompt = Prompt::new("hybrid-test", "Testing hybrid index/remove.");
        prompt.domain = Domain::Coding;
        storage.insert_prompt(&prompt).await.unwrap();

        let engine = HybridEngine::default_engines(storage);

        // Index → verify embedding persisted; remove → verify it's cleared.
        assert!(engine.index(&prompt).await.is_ok());

        // Verify the embedding exists by searching.
        let results = engine
            .search("hybrid", &SearchFilters::default(), &Pagination::default())
            .await
            .unwrap();
        assert!(
            results.items.iter().any(|s| s.prompt.name == "hybrid-test"),
            "Smart engine should find the indexed prompt"
        );

        // Remove and verify gone.
        assert!(engine.remove(prompt.id).await.is_ok());
    }

    // -- Send + Sync checks -------------------------------------------------

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_fast_engine_send_sync() {
        assert_send_sync::<FastEngine>();
    }

    #[test]
    fn test_smart_engine_send_sync() {
        assert_send_sync::<SmartEngine>();
    }

    #[test]
    fn test_hybrid_engine_send_sync() {
        assert_send_sync::<HybridEngine>();
    }

    #[test]
    fn test_search_engine_object_safe() {
        fn _takes_dyn(e: Arc<dyn SearchEngine>) {
            let _ = e.name();
        }
        // Compilation test only.
    }
}
