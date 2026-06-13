#![forbid(unsafe_code)]

//! Integration tests for offline mode feature.

use prompt_hub::{HubConfig, OfflineConfig, PromptHub};

#[tokio::test]
async fn test_full_hub_offline_flow() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("prompthub.db");
    let hub = PromptHub::new(&db_path, HubConfig::default())
        .await
        .unwrap();

    // Create a prompt via the hub while online.
    let prompt = prompt_hub::Prompt::new("offline-test", "Hello offline world!");
    let _id = hub
        .register(
            prompt.clone(),
            &prompt_hub::AgentIdentity::local_operator("test"),
        )
        .await
        .unwrap();

    // Enable offline mode with default config (auto_sync=false, LWW strategy).
    hub.enable_offline_mode(OfflineConfig::default()).unwrap();

    // Sync status should be Offline.
    let status = hub.get_sync_status().unwrap();
    assert!(matches!(status, prompt_hub::offline::SyncStatus::Offline));

    // The offline store should be empty (sync hasn't run yet).
    let guard = hub.offlined().read().unwrap();
    let state = guard.as_ref().unwrap();
    assert_eq!(state.store.pending_push_count(), 0);

    drop(guard);

    // Enable offline mode again should return an error.
    assert!(hub.enable_offline_mode(OfflineConfig::default()).is_err());
}

#[tokio::test]
async fn test_sync_conflict_detection() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("prompthub.db");
    let hub = PromptHub::new(&db_path, HubConfig::default())
        .await
        .unwrap();

    // Create a prompt that will exist in both store and offline.
    let prompt = prompt_hub::Prompt::new("conflict-test", "system prompt");
    let _id = hub
        .register(
            prompt.clone(),
            &prompt_hub::AgentIdentity::local_operator("test"),
        )
        .await
        .unwrap();

    // Enable offline mode.
    hub.enable_offline_mode(OfflineConfig::default()).unwrap();

    // The sync should succeed without conflicts (the prompt was just created
    // and has no local changes yet).
    let status = hub.sync().await.unwrap();
    assert!(matches!(status, prompt_hub::offline::SyncStatus::Online));
}
