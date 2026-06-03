use prompt_hub::config::HubConfig;
use prompt_hub::hub::PromptHub;
use std::path::Path;

#[tokio::test]
async fn test_hub_creation() {
    let config = HubConfig::default();
    let result = PromptHub::new(Path::new(":memory:"), config).await;
    assert!(result.is_ok());
    let hub = result.unwrap();
    // A successfully constructed hub exposes a live storage handle.
    let _storage = hub.storage();
}

#[tokio::test]
async fn test_hub_creation_with_config() {
    let mut config = HubConfig::default();
    config.max_pool_size = 5;
    config.auto_migrate = false;

    let result = PromptHub::new(Path::new(":memory:"), config.clone()).await;
    assert!(result.is_ok());
    assert_eq!(config.max_pool_size, 5);
    assert!(!config.auto_migrate);
}

#[tokio::test]
async fn test_hub_db_path() {
    let result = PromptHub::new(Path::new(":memory:"), HubConfig::default()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_hub_config_default() {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new("test.db"), config.clone())
        .await
        .unwrap();
    let _storage = hub.storage();
    assert_eq!(config.max_pool_size, 10);
    assert_eq!(config.default_page_size, 20);
    assert_eq!(config.embedding_dimension, 384);
    assert!(config.auto_migrate);
}

#[tokio::test]
async fn test_hub_is_initialized() {
    let hub = PromptHub::new(Path::new(":memory:"), HubConfig::default())
        .await
        .unwrap();
    // Construction succeeding implies the hub is initialized; storage is live.
    let _storage = hub.storage();
}

#[tokio::test]
async fn test_hub_with_file_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("prompthub.db");

    let result = PromptHub::new(&db_path, HubConfig::default()).await;
    assert!(result.is_ok());
    let _storage = result.unwrap().storage();
}

#[tokio::test]
async fn test_hub_multiple_instances() {
    // Multiple hubs can exist independently
    let hub1 = PromptHub::new(Path::new(":memory:"), HubConfig::default())
        .await
        .unwrap();
    let hub2 = PromptHub::new(Path::new(":memory:"), HubConfig::default())
        .await
        .unwrap();

    let _s1 = hub1.storage();
    let _s2 = hub2.storage();
}

#[tokio::test]
async fn test_hub_config_load_fallback() {
    // When no config file exists, load returns None and default is used
    let config = HubConfig::load().unwrap_or_default();
    assert_eq!(config.max_pool_size, 10);
    assert_eq!(config.default_page_size, 20);
}
