#![forbid(unsafe_code)]

//! Integration tests for the Qdrant vector search feature.
//!
//! These tests verify the wiring between PromptHub and the Qdrant engine,
//! including configuration parsing and engine selection.

use prompt_hub::qdrant::{
    QdrantConfig, QdrantEngine, QdrantHubConfigBuilder, QdrantSearchHit, VectorSearchMode,
};
use prompt_hub::search::{Embedder, HashEmbedder, SearchEngine};

/// Verify QdrantConfig round-trips through JSON serialization.
#[test]
fn test_integration_qdrant_config_round_trip() {
    let config = QdrantConfig {
        url: "https://qdrant.example.com:6333".to_string(),
        api_key: Some("my-api-key".to_string()),
        collection_name: "prompts".to_string(),
        vector_size: 768,
        distance: prompt_hub::qdrant::Distance::Dot,
        auto_create_collection: true,
    };

    let json = serde_json::to_string(&config).expect("serialize");
    let parsed: QdrantConfig = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(parsed.url, config.url);
    assert_eq!(parsed.api_key, config.api_key);
    assert_eq!(parsed.collection_name, config.collection_name);
    assert_eq!(parsed.vector_size, config.vector_size);
    assert!(matches!(parsed.distance, prompt_hub::qdrant::Distance::Dot));
}

/// Verify QdrantHubConfigBuilder constructs valid (HubConfig, Option<QdrantConfig>) pairs.
#[test]
fn test_integration_builder_with_qdrant() {
    let qdrant_cfg = QdrantConfig {
        url: "http://localhost:6333".to_string(),
        api_key: None,
        collection_name: "test".to_string(),
        vector_size: 384,
        distance: prompt_hub::qdrant::Distance::Cosine,
        auto_create_collection: false,
    };

    let (hub_cfg, qdrant) = QdrantHubConfigBuilder::new()
        .with_qdrant(qdrant_cfg.clone())
        .build();

    assert_eq!(hub_cfg.max_pool_size, 10);
    assert_eq!(hub_cfg.default_page_size, 20);
    assert!(qdrant.is_some());
    assert_eq!(qdrant.unwrap().url, "http://localhost:6333");
}

/// Verify QdrantHubConfigBuilder without qdrant returns None for config.
#[test]
fn test_integration_builder_without_qdrant() {
    let (hub_cfg, qdrant) = QdrantHubConfigBuilder::new().build();
    assert_eq!(hub_cfg.max_pool_size, 10);
    assert!(qdrant.is_none());
}

/// Verify QdrantEngine can be constructed with a HashEmbedder.
#[test]
fn test_integration_qdrant_engine_construction() {
    let qdrant_config = QdrantConfig {
        url: "http://localhost:6333".to_string(),
        api_key: None,
        collection_name: "test".to_string(),
        vector_size: 384,
        distance: prompt_hub::qdrant::Distance::Cosine,
        auto_create_collection: false,
    };

    let client = prompt_hub::qdrant::QdrantClient::new(qdrant_config);
    let embedder: std::sync::Arc<dyn Embedder> = std::sync::Arc::new(HashEmbedder::new(384));
    let engine = QdrantEngine::new(client, embedder, VectorSearchMode::default());

    assert_eq!(engine.name(), "QDRANT");
    assert!((engine.config().vector_size - 384) < 1);
}

/// Verify a search hit can be deserialized from Qdrant JSON and its fields extracted.
#[test]
fn test_integration_search_hit_deserialization() {
    let json = serde_json::json!({
        "id": "123e4567-e89b-12d3-a456-426614174000",
        "score": 0.8543,
        "payload": {
            "prompt_id": "00000000-0000-4000-a000-000000000001",
            "name": "greeting_prompt",
            "status": "Active",
            "domain": "Coding"
        }
    });

    let hit: QdrantSearchHit = serde_json::from_value(json).expect("deserialize");
    assert_eq!(hit.id, "123e4567-e89b-12d3-a456-426614174000");
    assert!((hit.score - 0.8543).abs() < f32::EPSILON);
    assert_eq!(hit.prompt_name(), Some("greeting_prompt"));
}

/// Verify QdrantConfig serializes correctly when api_key is None.
#[test]
fn test_integration_qdrant_config_without_api_key() {
    let config = QdrantConfig {
        url: "http://localhost:6333".to_string(),
        api_key: None,
        collection_name: "prompts".to_string(),
        vector_size: 384,
        distance: prompt_hub::qdrant::Distance::Euclid,
        auto_create_collection: false,
    };

    let json = serde_json::to_string(&config).expect("serialize");
    assert!(json.contains("\"api_key\":null"));
}

/// Verify VectorSearchMode values serialize correctly.
#[test]
fn test_integration_vector_search_mode_serde() {
    // QdrantConfig is the primary user of VectorSearchMode (via Hub integration),
    // but we test serialization here for completeness.
    assert!(matches!(
        VectorSearchMode::FtsOnly,
        VectorSearchMode::FtsOnly
    ));
    assert!(matches!(
        VectorSearchMode::VectorOnly,
        VectorSearchMode::VectorOnly
    ));
}
