#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Embedding backend selection.
///
/// Controls which `Embedder` implementation SmartEngine uses for vector generation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbedderBackend {
    /// Deterministic hash-based embedding (fast, reproducible — ideal for tests/dev).
    #[default]
    Hash,
    /// ONNX Runtime inference via the `ort` crate (real ML models).
    /// Requires the `smart-ort` feature flag.
    OnnxRuntime,
    /// Qdrant vector store — requires the `qdrant` feature flag.
    #[cfg(feature = "qdrant")]
    Qdrant,
}

/// Hub configuration for database, search, and runtime settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubConfig {
    /// Maximum database connection pool size
    pub max_pool_size: usize,
    /// Default page size for paginated results
    pub default_page_size: usize,
    /// Maximum page size allowed
    pub max_page_size: usize,
    /// Path to configuration directory
    pub config_dir: Option<PathBuf>,
    /// Enable auto-migration on startup
    pub auto_migrate: bool,
    /// Search result default limit
    pub default_search_limit: usize,
    /// Maximum search results
    pub max_search_limit: usize,
    /// Embedding model name (e.g. "sentence-transformers/all-MiniLM-L6-v2").
    pub embedding_model: String,
    /// Embedding dimension (must match the selected model).
    pub embedding_dimension: usize,
    /// Which embedder backend to use for vector generation.
    #[serde(default)]
    pub embedding_backend: EmbedderBackend,
    /// Qdrant connection configuration (optional). When present, enables
    /// vector search backed by a remote Qdrant cluster.
    #[cfg(feature = "qdrant")]
    #[serde(default)]
    pub qdrant_config: Option<crate::qdrant::QdrantConfig>,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            max_pool_size: 10,
            default_page_size: 20,
            max_page_size: 100,
            config_dir: None,
            auto_migrate: true,
            default_search_limit: 10,
            max_search_limit: 100,
            embedding_model: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            embedding_dimension: 384,
            embedding_backend: EmbedderBackend::default(),
            #[cfg(feature = "qdrant")]
            qdrant_config: None,
        }
    }
}

impl HubConfig {
    /// Load configuration from default locations.
    ///
    /// Attempts to load from (in order):
    /// 1. `PROMPTHUB_CONFIG` environment variable
    /// 2. XDG config directory (`~/.config/prompthub/config.toml`)
    /// 3. Current directory (`./prompthub.toml`)
    ///
    /// Falls back to [`Default`] if no config file is found.
    pub fn load() -> Option<Self> {
        // Try environment variable first
        if let Ok(path_str) = std::env::var("PROMPTHUB_CONFIG") {
            let path = PathBuf::from(path_str);
            if path.exists()
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Ok(config) = toml::from_str(&content)
            {
                return Some(config);
            }
        }

        // Try XDG config directory
        if let Some(config_dir) = dirs::config_dir() {
            let path = config_dir.join("prompthub").join("config.toml");
            if path.exists()
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Ok(config) = toml::from_str(&content)
            {
                return Some(config);
            }
        }

        // Try current directory
        let local = PathBuf::from("prompthub.toml");
        if local.exists()
            && let Ok(content) = std::fs::read_to_string(&local)
            && let Ok(config) = toml::from_str(&content)
        {
            return Some(config);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HubConfig::default();
        assert_eq!(config.max_pool_size, 10);
        assert_eq!(config.default_page_size, 20);
        assert_eq!(config.max_page_size, 100);
        assert!(config.auto_migrate);
        assert_eq!(config.embedding_dimension, 384);
        assert_eq!(config.embedding_backend, EmbedderBackend::Hash);
    }

    #[test]
    fn test_embedder_backend_serialization() {
        let toml_hash = r#"embedding_backend = "onnx_runtime""#;
        let parsed: serde_json::Value = toml::from_str(toml_hash).unwrap();
        assert_eq!(
            parsed.get("embedding_backend").unwrap().as_str().unwrap(),
            "onnx_runtime"
        );

        // Hash is the default — serializes and deserializes cleanly
        let cfg = HubConfig::default();
        let serialized = toml::to_string(&cfg).unwrap();
        let deserialized: HubConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.embedding_backend, EmbedderBackend::Hash);
    }

    #[test]
    fn test_config_load_none() {
        // When no config exists, load() returns None
        // (assuming we don't have a config file in the test environment)
        let _ = HubConfig::load();
        // Just ensure it doesn't panic
    }
}
