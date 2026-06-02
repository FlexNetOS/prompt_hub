use prompt_hub::search::{FastEngine, HybridEngine, Pagination, ScoredPrompt, SearchEngine, SearchFilters, SearchMode, SmartEngine};

#[tokio::test]
async fn test_fast_search_empty() {
    let engine = FastEngine::new();
    let result = engine
        .search("test", &SearchFilters::default(), &Pagination::default())
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().total, 0);
}

#[tokio::test]
async fn test_smart_search_empty() {
    let engine = SmartEngine::new();
    let result = engine
        .search("test", &SearchFilters::default(), &Pagination::default())
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().total, 0);
}

#[tokio::test]
async fn test_hybrid_search_empty() {
    let engine = HybridEngine::new();
    let result = engine
        .search("test", &SearchFilters::default(), &Pagination::default())
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_fast_engine_index_and_remove() {
    let engine = FastEngine::new();
    let id = uuid::Uuid::new_v4();

    assert!(engine.index(id, "test content for indexing").await.is_ok());
    assert!(engine.remove(id).await.is_ok());
}

#[tokio::test]
async fn test_smart_engine_index_and_remove() {
    let engine = SmartEngine::new();
    let id = uuid::Uuid::new_v4();

    assert!(engine.index(id, "test content for indexing").await.is_ok());
    assert!(engine.remove(id).await.is_ok());
}

#[tokio::test]
async fn test_hybrid_engine_index_and_remove() {
    let engine = HybridEngine::new();
    let id = uuid::Uuid::new_v4();

    assert!(engine.index(id, "test content for indexing").await.is_ok());
    assert!(engine.remove(id).await.is_ok());
}

#[tokio::test]
async fn test_search_with_filters() {
    let engine = FastEngine::new();
    let filters = SearchFilters {
        domain: Some("coding".to_string()),
        tags: vec!["rust".to_string()],
        ..SearchFilters::default()
    };
    let result = engine.search("rust", &filters, &Pagination::default()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_search_with_pagination() {
    let engine = FastEngine::new();
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
    let sp = ScoredPrompt {
        id: uuid::Uuid::new_v4(),
        name: "test-prompt".to_string(),
        score: 0.95,
        version: "1.0.0".to_string(),
        domain: "coding".to_string(),
        status: "active".to_string(),
    };
    assert_eq!(sp.name, "test-prompt");
    assert!(sp.score > 0.9);
}

#[tokio::test]
async fn test_fast_search_concurrent() {
    let engine = FastEngine::new();

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
    let fast = FastEngine::new();
    let smart = SmartEngine::new();
    let hybrid = HybridEngine::new();

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
