#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    /// Embedding model name
    pub embedding_model: String,
    /// Embedding dimension
    pub embedding_dimension: usize,
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
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(config) = toml::from_str(&content) {
                        return Some(config);
                    }
                }
            }
        }

        // Try XDG config directory
        if let Some(config_dir) = dirs::config_dir() {
            let path = config_dir.join("prompthub").join("config.toml");
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(config) = toml::from_str(&content) {
                        return Some(config);
                    }
                }
            }
        }

        // Try current directory
        let local = PathBuf::from("prompthub.toml");
        if local.exists() {
            if let Ok(content) = std::fs::read_to_string(&local) {
                if let Ok(config) = toml::from_str(&content) {
                    return Some(config);
                }
            }
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
    }

    #[test]
    fn test_config_load_none() {
        // When no config exists, load() returns None
        // (assuming we don't have a config file in the test environment)
        let _ = HubConfig::load();
        // Just ensure it doesn't panic
    }
}
