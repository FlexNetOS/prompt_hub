#![forbid(unsafe_code)]

//! Qdrant-backed vector search engine for PromptHub.
//!
//! Provides a [`QdrantClient`] that speaks the Qdrant REST API and a
//! [`QdrantEngine`] that implements the existing [`crate::search::SearchEngine`]
//! trait by delegating embedding computation to a pluggable [`Embedder`].
//!
//! The module is gated behind the `qdrant` feature flag.

use crate::config::HubConfig;
use crate::error::{HubError, Result};
use crate::search::Embedder;
use crate::search::SearchEngine as _SearchEngineTrait;
use crate::{Pagination, Prompt, PromptMeta, PromptMetrics, ScoredPrompt, SearchFilters, Status};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Qdrant connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantConfig {
    /// Base URL of the Qdrant server (e.g. `"https://qdrant.example.com:6333"`).
    pub url: String,
    /// API key for authentication (optional — some deployments use no auth).
    pub api_key: Option<String>,
    /// Name of the collection to store/search vectors in.
    pub collection_name: String,
    /// Dimension of the embedding vectors — must match the model dimension.
    pub vector_size: usize,
    /// Distance metric for vector comparison.
    pub distance: Distance,
    /// If true, create the collection automatically on first use.
    pub auto_create_collection: bool,
}

/// Distance metric for vector comparison.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Distance {
    /// Cosine similarity (default).
    #[default]
    Cosine,
    /// Dot product similarity.
    Dot,
    /// Euclidean distance.
    Euclid,
}

impl Distance {
    fn to_qdrant_string(self) -> &'static str {
        match self {
            Distance::Cosine => "Cosine",
            Distance::Dot => "Dot",
            Distance::Euclid => "Euclid",
        }
    }
}

/// A Qdrant search result hit.
#[derive(Debug, Clone, Deserialize)]
pub struct QdrantSearchHit {
    /// The point ID as returned by Qdrant.
    pub id: String,
    /// Similarity score (higher = more similar).
    pub score: f32,
    /// Attached JSON payload containing prompt fields.
    pub payload: serde_json::Value,
}

impl QdrantSearchHit {
    /// Extract the prompt UUID from the hit's payload.
    fn prompt_id(&self) -> Option<Uuid> {
        self.payload
            .get("prompt_id")?
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
    }

    /// Extract the prompt name from the hit's payload.
    pub fn prompt_name(&self) -> Option<&str> {
        self.payload.get("name")?.as_str()
    }

    /// Extract the status from the hit's payload.
    fn status_str(&self) -> Option<&str> {
        self.payload.get("status")?.as_str()
    }
}

// ---------------------------------------------------------------------------
// HTTP client
// ---------------------------------------------------------------------------

/// HTTP client wrapper around the Qdrant REST API.
#[derive(Debug)]
pub struct QdrantClient {
    config: QdrantConfig,
    client: reqwest::Client,
}

impl QdrantClient {
    /// Create a new client with the given configuration.
    pub fn new(config: QdrantConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    // ── helpers ----------------------------------------------------------

    fn base_url(&self) -> String {
        self.config.url.clone()
    }

    /// Build the collection path segment.
    fn collection_path(&self) -> String {
        format!("collections/{}", self.config.collection_name)
    }

    /// Attach API key header if configured.
    fn headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(ref key) = self.config.api_key {
            headers.insert(
                "api-key",
                key.parse().expect("invalid api-key header value"),
            );
        }
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers
    }

    // ── public API -------------------------------------------------------

    /// Health check — GET /healthz.
    pub async fn health(&self) -> Result<bool> {
        let url = format!("{}/healthz", self.base_url());
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| HubError::Network(format!("Qdrant health check failed: {e}")))?;

        Ok(resp.status().is_success())
    }

    /// Ensure the target collection exists; create it if not.
    ///
    /// Skips creation when `auto_create_collection` is false.
    pub async fn ensure_collection(&self) -> Result<()> {
        if !self.config.auto_create_collection {
            return Ok(());
        }

        // Check if collection already exists.
        let url = format!("{}/{}/info", self.base_url(), self.collection_path());
        let resp = self.client.get(&url).headers(self.headers()).send().await;

        // If it exists we're done. On failure (not found, network error, ...) proceed to create.
        if resp.is_ok() {
            debug!(collection = %self.config.collection_name, "Collection already exists");
            return Ok(());
        }

        // Create the collection with the specified vector config.
        let create_body = serde_json::json!({
            "vectors": {
                "size": self.config.vector_size,
                "distance": self.config.distance.to_qdrant_string(),
            }
        });

        let url = format!(
            "{}/collections/{}",
            self.base_url(),
            self.config.collection_name
        );
        let resp = self
            .client
            .put(&url)
            .headers(self.headers())
            .json(&create_body)
            .send()
            .await
            .map_err(|e| HubError::Network(format!("Qdrant create collection: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(HubError::Internal(format!(
                "Failed to create Qdrant collection: {body}"
            )));
        }

        info!(
            collection = %self.config.collection_name,
            "Created Qdrant collection"
        );
        Ok(())
    }

    /// Upsert a single point with vector + payload.
    pub async fn upsert(
        &self,
        point_id: Uuid,
        vector: &[f32],
        payload: serde_json::Value,
    ) -> Result<()> {
        let points = serde_json::json!([{
            "id": point_id.to_string(),
            "vector": vector,
            "payload": payload,
        }]);

        let body = serde_json::json!({
            "points": points,
            "wait": true,
        });

        let url = format!(
            "{}/{}/points?wait=true",
            self.base_url(),
            self.collection_path()
        );
        let resp = self
            .client
            .put(&url)
            .headers(self.headers())
            .json(&body)
            .send()
            .await
            .map_err(|e| HubError::Network(format!("Qdrant upsert: {e}")))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(HubError::Internal(format!("Qdrant upsert failed: {text}")));
        }

        debug!(point_id = %point_id, "Upserted point to Qdrant");
        Ok(())
    }

    /// Delete points by their IDs.
    pub async fn delete_points(&self, point_ids: &[Uuid]) -> Result<()> {
        if point_ids.is_empty() {
            return Ok(());
        }

        let ids: Vec<String> = point_ids.iter().map(|id| id.to_string()).collect();

        let body = serde_json::json!({
            "points": ids,
            "wait": true,
        });

        // Qdrant deletes via a POST to /collections/{name}/points/ids endpoint.
        let url = format!(
            "{}/{}/points/ids?wait=true",
            self.base_url(),
            self.collection_path()
        );
        let resp = self
            .client
            .post(&url)
            .headers(self.headers())
            .json(&body)
            .send()
            .await
            .map_err(|e| HubError::Network(format!("Qdrant delete points: {e}")))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(HubError::Internal(format!("Qdrant delete failed: {text}")));
        }

        debug!(count = point_ids.len(), "Deleted points from Qdrant");
        Ok(())
    }

    /// Search for the top-*limit* nearest neighbours to *vector*.
    pub async fn search(&self, vector: &[f32], limit: usize) -> Result<Vec<QdrantSearchHit>> {
        let body = serde_json::json!({
            "vector": vector,
            "limit": limit,
            "with_payload": true,
        });

        let url = format!(
            "{}/{}/points/search",
            self.base_url(),
            self.collection_path()
        );
        let resp = self
            .client
            .post(&url)
            .headers(self.headers())
            .json(&body)
            .send()
            .await
            .map_err(|e| HubError::Network(format!("Qdrant search: {e}")))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(HubError::SearchError(format!(
                "Qdrant search failed: {text}"
            )));
        }

        #[derive(Deserialize)]
        struct SearchResponse {
            result: Vec<QdrantSearchHit>,
        }

        let parsed: SearchResponse = resp.json().await.map_err(|e| {
            HubError::Serialization(format!("Failed to parse Qdrant search result: {e}"))
        })?;

        Ok(parsed.result)
    }
}

// ---------------------------------------------------------------------------
// Search mode
// ---------------------------------------------------------------------------

/// Search mode for the vector store backend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VectorSearchMode {
    /// FTS-only — bypass vector search entirely.
    FtsOnly,
    /// Vector-only — use Qdrant only, no keyword matching.
    VectorOnly,
    /// Hybrid: the `f64` is the weight for the vector score (1-weight goes to FTS).
    Hybrid(f64),
}

impl Default for VectorSearchMode {
    fn default() -> Self {
        VectorSearchMode::Hybrid(0.6)
    }
}

// ---------------------------------------------------------------------------
// Search engine implementing the existing SearchEngine trait
// ---------------------------------------------------------------------------

/// Qdrant-backed search engine.
///
/// Wraps a [`QdrantClient`] and an [`Embedder`], delegating embedding
/// computation to the embedder and storing / searching vectors via Qdrant.
#[derive(Debug)]
pub struct QdrantEngine {
    client: QdrantClient,
    embedder: Arc<dyn Embedder>,
    mode: VectorSearchMode,
}

impl QdrantEngine {
    /// Create a new `QdrantEngine`.
    pub fn new(client: QdrantClient, embedder: Arc<dyn Embedder>, mode: VectorSearchMode) -> Self {
        Self {
            client,
            embedder,
            mode,
        }
    }

    /// Access the underlying Qdrant configuration.
    pub fn config(&self) -> &QdrantConfig {
        &self.client.config
    }

    /// Access the embedder.
    pub fn embedder(&self) -> &Arc<dyn Embedder> {
        &self.embedder
    }

    /// Build an embedding for a single text using the configured embedder.
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        let batch = self.embedder.embed(&[text.to_string()]).await?;
        batch
            .into_iter()
            .next()
            .ok_or_else(|| HubError::SearchError("embedder returned empty batch".into()))
    }

    /// Reconstruct a minimal [`crate::Prompt`] from a Qdrant payload.
    fn payload_to_prompt(payload: &serde_json::Value) -> Prompt {
        let id_str = payload
            .get("prompt_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let id = Uuid::parse_str(id_str).unwrap_or_else(|_| Uuid::default());

        Prompt {
            id,
            name: payload
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            version: semver::Version::new(1, 0, 0),
            status: parse_status(payload.get("status").and_then(|v| v.as_str())),
            system_prompt: payload
                .get("system_prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            user_template: payload
                .get("user_template")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            required_vars: vec![],
            domain: parse_domain(payload.get("domain").and_then(|v| v.as_str())),
            tags: payload
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            target_roles: vec![],
            metadata: PromptMeta::default(),
            metrics: PromptMetrics::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            author: crate::models::AgentIdentity::default(),
            deleted_at: None,
            generation_params: None,
            locale: None,
            multimodal: None,
        }
    }

    /// Convert raw Qdrant search hits into a paginated result.
    fn hits_to_paginated(
        hits: Vec<QdrantSearchHit>,
        pagination: &Pagination,
    ) -> crate::Paginated<ScoredPrompt> {
        let mut scored: Vec<ScoredPrompt> = Vec::new();

        for hit in hits {
            let prompt = Self::payload_to_prompt(&hit.payload);
            scored.push(ScoredPrompt {
                prompt,
                score: hit.score.abs(),
                matched_field: Some("vector".to_string()),
            });
        }

        // Sort by score descending.
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let total = scored.len();
        let start = (pagination.page.saturating_sub(1)).min(total) * pagination.per_page;
        let items = scored
            .into_iter()
            .skip(start)
            .take(pagination.per_page)
            .collect();

        crate::Paginated {
            items,
            total,
            page: pagination.page,
            per_page: pagination.per_page,
        }
    }
}

impl _SearchEngineTrait for QdrantEngine {
    #[tracing::instrument(skip(self, _filters, pagination))]
    fn search<'a>(
        &'a self,
        query: &'a str,
        _filters: &'a SearchFilters,
        pagination: &'a Pagination,
    ) -> Pin<Box<dyn Future<Output = Result<crate::Paginated<ScoredPrompt>>> + Send + 'a>> {
        Box::pin(async move {
            debug!(mode = ?self.mode, "QDRANT search: '{}' filters={:?}", query, _filters);

            match self.mode {
                VectorSearchMode::FtsOnly => {
                    // Return empty — vector engine is bypassed. The hub should
                    // have a FAST engine in the hybrid fallback for this path.
                    Ok(crate::Paginated {
                        items: Vec::new(),
                        total: 0,
                        page: pagination.page,
                        per_page: pagination.per_page,
                    })
                }

                VectorSearchMode::VectorOnly => {
                    let query_vec = self.embed_text(query).await?;
                    let hits = self
                        .client
                        .search(&query_vec, pagination.per_page.max(1))
                        .await?;
                    Ok(Self::hits_to_paginated(hits, pagination))
                }

                VectorSearchMode::Hybrid(weight) => {
                    // Embed the query and search Qdrant.
                    let query_vec = self.embed_text(query).await?;
                    let hits = self
                        .client
                        .search(&query_vec, pagination.per_page.max(1))
                        .await?;

                    let paginated = Self::hits_to_paginated(hits, pagination);

                    // Apply hybrid weighting: scale vector scores by the weight factor.
                    let items: Vec<ScoredPrompt> = paginated
                        .items
                        .into_iter()
                        .map(|mut sp| {
                            sp.score *= weight as f32;
                            sp
                        })
                        .collect();

                    Ok(crate::Paginated {
                        items,
                        total: paginated.total,
                        page: pagination.page,
                        per_page: pagination.per_page,
                    })
                }
            }
        })
    }

    fn index<'a>(
        &'a self,
        prompt: &'a Prompt,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // Compose embedding text from prompt content fields (same as SmartEngine).
            let text = format!(
                "{}\n{}\n{}",
                prompt.name, prompt.system_prompt, prompt.user_template
            );

            if text.is_empty() {
                warn!("QDRANT index: empty prompt — skipping");
                return Ok(());
            }

            // Embed via pluggable backend.
            let vector = self.embed_text(&text).await?;

            // Build payload from prompt fields.
            let payload = serde_json::json!({
                "prompt_id": prompt.id.to_string(),
                "name": prompt.name,
                "system_prompt": prompt.system_prompt,
                "user_template": prompt.user_template,
                "status": format!("{:?}", prompt.status),
                "domain": format!("{:?}", prompt.domain),
                "tags": prompt.tags.clone(),
            });

            self.client.upsert(prompt.id, &vector, payload).await?;

            info!(prompt_id = %prompt.id, dim = %vector.len(), "QDRANT index: prompt embedded");
            Ok(())
        })
    }

    fn remove(&self, prompt_id: Uuid) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            self.client.delete_points(&[prompt_id]).await?;
            debug!("QDRANT remove: id={prompt_id} — point deleted");
            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "QDRANT"
    }
}

// ---------------------------------------------------------------------------
// Helper utilities
// ---------------------------------------------------------------------------

fn parse_status(s: Option<&str>) -> Status {
    match s.unwrap_or("Active") {
        "Active" => Status::Active,
        "Draft" => Status::Draft,
        "Archived" => Status::Archived,
        _other => Status::Active, // default to Active for unknowns
    }
}

fn parse_domain(s: Option<&str>) -> crate::Domain {
    match s.unwrap_or("General") {
        "Coding" => crate::Domain::Coding,
        "DevOps" => crate::Domain::DevOps,
        "Security" => crate::Domain::Security,
        "Analysis" => crate::Domain::Analysis,
        "Design" => crate::Domain::Design,
        "DataScience" => crate::Domain::DataScience,
        "Testing" => crate::Domain::Testing,
        "Documentation" => crate::Domain::Documentation,
        "Writing" => crate::Domain::Writing,
        _ => crate::Domain::General,
    }
}

// ---------------------------------------------------------------------------
// HubConfig extension — qdrant_config field
// ---------------------------------------------------------------------------

/// Builder for constructing a HubConfig with Qdrant support.
#[derive(Debug, Clone)]
pub struct QdrantHubConfigBuilder {
    config: HubConfig,
    qdrant_config: Option<QdrantConfig>,
}

impl QdrantHubConfigBuilder {
    /// Create a new builder starting from the default HubConfig.
    pub fn new() -> Self {
        Self {
            config: HubConfig::default(),
            qdrant_config: None,
        }
    }

    /// Set the Qdrant configuration.
    pub fn with_qdrant(mut self, config: QdrantConfig) -> Self {
        self.qdrant_config = Some(config);
        self
    }

    /// Build — returns `(HubConfig, Option<QdrantConfig>)`.
    pub fn build(self) -> (HubConfig, Option<QdrantConfig>) {
        (self.config, self.qdrant_config)
    }
}

impl Default for QdrantHubConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod qdrant_tests {
    use super::*;

    /// Create a QdrantConfig pointing at localhost for testing.
    fn test_config() -> QdrantConfig {
        QdrantConfig {
            url: "http://localhost:6333".to_string(),
            api_key: None,
            collection_name: "prompthub_test".to_string(),
            vector_size: 384,
            distance: Distance::Cosine,
            auto_create_collection: false,
        }
    }

    // -- QdrantConfig -------------------------------------------------------

    /// Unit: QdrantConfig serializes to JSON and round-trips.
    #[test]
    fn test_qdrant_config_serialization() {
        let config = test_config();
        let json = serde_json::to_string(&config).expect("serialize QdrantConfig");
        let parsed: QdrantConfig = serde_json::from_str(&json).expect("deserialize QdrantConfig");
        assert_eq!(parsed.url, config.url);
        assert_eq!(parsed.collection_name, config.collection_name);
        assert_eq!(parsed.vector_size, config.vector_size);
        assert_eq!(parsed.distance, config.distance);
        assert!(!parsed.auto_create_collection);
    }

    // -- Distance -----------------------------------------------------------

    /// Unit: Distance enum serializes to snake_case and round-trips.
    #[test]
    fn test_distance_serialization() {
        for (dist, expected) in [
            (Distance::Cosine, "cosine"),
            (Distance::Dot, "dot"),
            (Distance::Euclid, "euclid"),
        ] {
            let json = serde_json::to_string(&dist).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));

            let parsed: Distance = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, dist);
        }
    }

    // -- QdrantSearchHit ----------------------------------------------------

    /// Unit: search hit extracts fields from payload.
    #[test]
    fn test_search_hit_payload_extraction() {
        let payload = serde_json::json!({
            "prompt_id": "00000000-0000-4000-a000-000000000001",
            "name": "test_prompt",
            "status": "Active",
            "domain": "Coding",
        });
        let hit = QdrantSearchHit {
            id: "1".to_string(),
            score: 0.85,
            payload,
        };
        assert_eq!(
            hit.prompt_id().unwrap().to_string(),
            "00000000-0000-4000-a000-000000000001"
        );
        assert_eq!(hit.prompt_name(), Some("test_prompt"));
        assert_eq!(hit.status_str(), Some("Active"));
    }

    /// Unit: missing payload fields return None gracefully.
    #[test]
    fn test_search_hit_missing_fields() {
        let hit = QdrantSearchHit {
            id: "2".to_string(),
            score: 0.5,
            payload: serde_json::Value::Object(serde_json::Map::new()),
        };
        assert!(hit.prompt_id().is_none());
        assert!(hit.prompt_name().is_none());
        assert!(hit.status_str().is_none());
    }

    // -- QdrantClient -------------------------------------------------------

    /// Unit: client construction.
    #[test]
    fn test_qdrant_client_new() {
        let config = test_config();
        let client = QdrantClient::new(config.clone());
        assert_eq!(client.config.url, "http://localhost:6333");
        assert_eq!(client.config.collection_name, "prompthub_test");
    }

    /// Unit: QdrantConfig with api_key serializes correctly.
    #[test]
    fn test_qdrant_config_with_api_key() {
        let config = QdrantConfig {
            url: "https://qdrant.example.com".to_string(),
            api_key: Some("secret-key".to_string()),
            collection_name: "prod".to_string(),
            vector_size: 768,
            distance: Distance::Dot,
            auto_create_collection: true,
        };
        let json = serde_json::to_string(&config).expect("serialize with api_key");
        assert!(json.contains("\"secret-key\""));
    }

    // -- VectorSearchMode ---------------------------------------------------

    /// Unit: VectorSearchMode defaults to Hybrid(0.6).
    #[test]
    fn test_vector_search_mode_default() {
        let mode = VectorSearchMode::default();
        assert!(matches!(mode, VectorSearchMode::Hybrid(w) if (w - 0.6).abs() < f64::EPSILON));
    }

    /// Unit: FtsOnly and VectorOnly match correctly.
    #[test]
    fn test_vector_search_mode_variants() {
        let fts = VectorSearchMode::FtsOnly;
        assert!(matches!(fts, VectorSearchMode::FtsOnly));

        let vec_only = VectorSearchMode::VectorOnly;
        assert!(matches!(vec_only, VectorSearchMode::VectorOnly));

        let hybrid = VectorSearchMode::Hybrid(0.4);
        assert!(matches!(hybrid, VectorSearchMode::Hybrid(w) if (w - 0.4).abs() < f64::EPSILON));
    }

    // -- QdrantHubConfigBuilder ---------------------------------------------

    /// Unit: builder defaults to no qdrant config.
    #[test]
    fn test_qdrant_builder_defaults() {
        let (config, qdrant) = QdrantHubConfigBuilder::new().build();
        assert_eq!(config.max_pool_size, 10);
        assert_eq!(config.default_page_size, 20);
        assert!(qdrant.is_none());
    }

    /// Unit: builder with qdrant config.
    #[test]
    fn test_qdrant_builder_with_config() {
        let (_config, qdrant) = QdrantHubConfigBuilder::new()
            .with_qdrant(test_config())
            .build();
        assert!(qdrant.is_some());
        assert_eq!(qdrant.unwrap().collection_name, "prompthub_test");
    }

    // -- QdrantEngine helper methods ----------------------------------------

    /// Unit: payload_to_prompt reconstructs Prompt from JSON.
    #[test]
    fn test_payload_to_prompt() {
        let payload = serde_json::json!({
            "prompt_id": "00000000-0000-4000-a000-000000000001",
            "name": "test_prompt",
            "system_prompt": "You are helpful.",
            "user_template": "Hello {name}",
            "status": "Active",
            "domain": "Coding",
            "tags": ["rust", "testing"],
        });
        let prompt = QdrantEngine::payload_to_prompt(&payload);
        assert_eq!(prompt.name, "test_prompt");
        assert_eq!(prompt.status, Status::Active);
    }

    /// Unit: hits_to_paginated sorts by score descending.
    #[test]
    fn test_hits_to_paginated_sorting() {
        let hits = vec![
            QdrantSearchHit {
                id: "1".to_string(),
                score: 0.3,
                payload: serde_json::json!({"name": "low"}),
            },
            QdrantSearchHit {
                id: "2".to_string(),
                score: 0.9,
                payload: serde_json::json!({"name": "high"}),
            },
            QdrantSearchHit {
                id: "3".to_string(),
                score: 0.6,
                payload: serde_json::json!({"name": "mid"}),
            },
        ];
        let result = QdrantEngine::hits_to_paginated(hits, &Pagination::default());
        assert_eq!(result.total, 3);
        assert_eq!(result.items[0].prompt.name, "high");
        assert_eq!(result.items[1].prompt.name, "mid");
        assert_eq!(result.items[2].prompt.name, "low");
    }

    /// Unit: hits_to_paginated respects pagination.
    #[test]
    fn test_hits_to_paginated_pagination() {
        let hits: Vec<QdrantSearchHit> = (0..5)
            .map(|i| QdrantSearchHit {
                id: i.to_string(),
                score: 0.9 - (i as f32 * 0.1),
                payload: serde_json::json!({"name": format!("p{}", i)}),
            })
            .collect();

        let mut pag = Pagination {
            page: 1,
            per_page: 2,
        };
        let result = QdrantEngine::hits_to_paginated(hits.clone(), &pag);
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.total, 5);

        pag.page = 2;
        let result2 = QdrantEngine::hits_to_paginated(hits, &pag);
        assert_eq!(result2.items.len(), 2);
    }

    // -- Integration tests (require a running Qdrant instance) ---------------

    /// Integration: verify health endpoint returns true for a live server.
    #[ignore = "requires a running Qdrant server on localhost:6333"]
    #[tokio::test]
    async fn test_health_integration() {
        let config = QdrantConfig {
            url: "http://localhost:6333".to_string(),
            api_key: None,
            collection_name: "health_test".to_string(),
            vector_size: 384,
            distance: Distance::Cosine,
            auto_create_collection: false,
        };
        let client = QdrantClient::new(config);

        // This test will return Ok(false) if Qdrant is not running.
        let healthy = client.health().await;
        assert!(healthy.is_ok());
    }

    /// Integration: ensure_collection does nothing when auto_create is false.
    #[tokio::test]
    async fn test_ensure_collection_noop() {
        let config = QdrantConfig {
            url: "http://localhost:6333".to_string(),
            api_key: None,
            collection_name: "noop_test".to_string(),
            vector_size: 384,
            distance: Distance::Cosine,
            auto_create_collection: false,
        };
        let client = QdrantClient::new(config);

        let result = client.ensure_collection().await;
        assert!(result.is_ok());
    }
}
