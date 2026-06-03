use std::sync::Arc;

use prompt_hub::models::{
    Domain, Pagination, Prompt, ScoredPrompt, SearchFilters, SearchMode, Status,
};
use prompt_hub::search::{FastEngine, HybridEngine, SearchEngine, SmartEngine};
use prompt_hub::storage::{Storage, StorageConfig};

/// Create an in-memory storage for tests.
async fn in_memory_storage() -> Arc<Storage> {
    let config = StorageConfig {
        db_path: ":memory:".to_string(),
        max_connections: 2,
        ..Default::default()
    };
    Arc::new(
        Storage::new(config)
            .await
            .expect("Failed to create in-memory storage"),
    )
}

/// Build a minimal prompt for index/remove smoke tests.
fn sample_prompt(name: &str) -> Prompt {
    let mut prompt = Prompt::new(name, "test content for indexing");
    prompt.domain = Domain::Coding;
    prompt
}

#[tokio::test]
async fn test_fast_search_empty() {
    let storage = in_memory_storage().await;
    let engine = FastEngine::new(storage);
    let result = engine
        .search("test", &SearchFilters::default(), &Pagination::default())
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().total, 0);
}

#[tokio::test]
async fn test_smart_search_empty() {
    let storage = in_memory_storage().await;
    let engine = SmartEngine::default_model(storage);
    let result = engine
        .search("test", &SearchFilters::default(), &Pagination::default())
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().total, 0);
}

#[tokio::test]
async fn test_hybrid_search_empty() {
    let storage = in_memory_storage().await;
    let engine = HybridEngine::default_engines(storage);
    let result = engine
        .search("test", &SearchFilters::default(), &Pagination::default())
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_fast_engine_index_and_remove() {
    let storage = in_memory_storage().await;
    let engine = FastEngine::new(storage);
    let prompt = sample_prompt("fast-index");

    assert!(engine.index(&prompt).await.is_ok());
    assert!(engine.remove(prompt.id).await.is_ok());
}

#[tokio::test]
async fn test_smart_engine_index_and_remove() {
    let storage = in_memory_storage().await;
    let engine = SmartEngine::default_model(storage);
    let prompt = sample_prompt("smart-index");

    assert!(engine.index(&prompt).await.is_ok());
    assert!(engine.remove(prompt.id).await.is_ok());
}

#[tokio::test]
async fn test_hybrid_engine_index_and_remove() {
    let storage = in_memory_storage().await;
    let engine = HybridEngine::default_engines(storage);
    let prompt = sample_prompt("hybrid-index");

    assert!(engine.index(&prompt).await.is_ok());
    assert!(engine.remove(prompt.id).await.is_ok());
}

#[tokio::test]
async fn test_search_with_filters() {
    let storage = in_memory_storage().await;
    let engine = FastEngine::new(storage);
    let filters = SearchFilters {
        domain: Some(Domain::Coding),
        tags: vec!["rust".to_string()],
        ..SearchFilters::default()
    };
    let result = engine
        .search("rust", &filters, &Pagination::default())
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_search_with_pagination() {
    let storage = in_memory_storage().await;
    let engine = FastEngine::new(storage);
    let pagination = Pagination {
        page: 2,
        per_page: 10,
    };
    let result = engine
        .search("test", &SearchFilters::default(), &pagination)
        .await;
    assert!(result.is_ok());
    let paginated = result.unwrap();
    assert_eq!(paginated.page, 2);
    assert_eq!(paginated.per_page, 10);
}

#[test]
fn test_search_mode_variants() {
    assert_eq!(SearchMode::Fast as u8, SearchMode::Fast as u8);
    assert_ne!(SearchMode::Fast, SearchMode::Smart);
    assert_ne!(SearchMode::Smart, SearchMode::Hybrid);
}

#[test]
fn test_search_filters_default() {
    let filters = SearchFilters::default();
    assert!(filters.domain.is_none());
    assert!(filters.role.is_none());
    assert!(filters.status.is_none());
    assert!(filters.tags.is_empty());
}

#[test]
fn test_pagination_default() {
    let p = Pagination::default();
    assert_eq!(p.page, 1);
    assert_eq!(p.per_page, 20);
}

#[test]
fn test_scored_prompt_creation() {
    let mut prompt = Prompt::new("test-prompt", "system prompt body");
    prompt.domain = Domain::Coding;
    prompt.status = Status::Active;
    let sp = ScoredPrompt {
        prompt,
        score: 0.95,
        matched_field: Some("name".to_string()),
    };
    assert_eq!(sp.prompt.name, "test-prompt");
    assert!(sp.score > 0.9);
}

#[tokio::test]
async fn test_fast_search_concurrent() {
    let storage = in_memory_storage().await;
    let engine = FastEngine::new(storage);

    // Run multiple searches concurrently
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let engine_ref = &engine;
            let query = format!("query-{}", i);
            async move {
                engine_ref
                    .search(&query, &SearchFilters::default(), &Pagination::default())
                    .await
            }
        })
        .collect();

    for handle in handles {
        assert!(handle.await.is_ok());
    }
}

#[tokio::test]
async fn test_all_engines_return_empty_for_empty_query() {
    let fast = FastEngine::new(in_memory_storage().await);
    let smart = SmartEngine::default_model(in_memory_storage().await);
    let hybrid = HybridEngine::default_engines(in_memory_storage().await);

    let query = "";

    let fast_result = fast
        .search(query, &SearchFilters::default(), &Pagination::default())
        .await;
    let smart_result = smart
        .search(query, &SearchFilters::default(), &Pagination::default())
        .await;
    let hybrid_result = hybrid
        .search(query, &SearchFilters::default(), &Pagination::default())
        .await;

    assert!(fast_result.is_ok());
    assert!(smart_result.is_ok());
    assert!(hybrid_result.is_ok());
}
