#![forbid(unsafe_code)]

/// Integration test: purge_now runs without error on empty DB.
#[cfg(feature = "auto-purge")]
#[tokio::test]
async fn test_purge_now_on_empty_db() {
    use prompt_hub::HubConfig;
    use std::path::Path;
    use std::time::Duration;

    let hub = prompt_hub::PromptHub::new(Path::new(":memory:"), HubConfig::default())
        .await
        .unwrap();

    // Start the daemon to initialize the engine, then configure it.
    hub.start_purge_daemon(prompt_hub::auto_purge::AutoPurgeConfig {
        enabled: true,
        interval: Duration::from_secs(1),
        policies: vec![prompt_hub::auto_purge::PurgePolicy {
            min_age_days: 0,
            condition: prompt_hub::auto_purge::PolicyCondition::DaysOld(0),
            action: prompt_hub::auto_purge::PurgeAction::Delete,
        }],
    })
    .await
    .unwrap();

    // Run purge — should succeed even with no prompts.
    let stats = hub.purge_now().await.unwrap();
    assert_eq!(stats.total_scanned, 0);
}

/// Integration test: archive policy condition construction works.
#[cfg(feature = "auto-purge")]
#[test]
fn test_archive_policy_condition_construction() {
    use prompt_hub::auto_purge::{PolicyCondition, PurgeAction, PurgePolicy};

    let policy = PurgePolicy {
        min_age_days: 30,
        condition: PolicyCondition::Tags(vec!["archive-me".into()]),
        action: PurgeAction::Archive("/tmp/archive".into()),
    };

    assert!(policy.min_age_days == 30);
    assert!(matches!(policy.action, PurgeAction::Archive(_)));
}

/// Integration test: get_purge_stats returns valid snapshot.
#[cfg(feature = "auto-purge")]
#[tokio::test]
async fn test_get_purge_stats() {
    use prompt_hub::HubConfig;
    use std::path::Path;

    let hub = prompt_hub::PromptHub::new(Path::new(":memory:"), HubConfig::default())
        .await
        .unwrap();

    // Initialize engine so get_purge_stats works.
    hub.start_purge_daemon(prompt_hub::auto_purge::AutoPurgeConfig::default())
        .await
        .unwrap();

    // Before any purge run, stats should be all zeros.
    let stats = hub.get_purge_stats().unwrap();
    assert_eq!(stats.total_scanned, 0);
    assert_eq!(stats.purged_count, 0);
}
