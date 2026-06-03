use chrono::Utc;
use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use prompt_hub::models::*;
use prompt_hub::{HubConfig, PromptHub};
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;

fn bench_insert_prompt(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("bench.db");

    let config = HubConfig::default();
    let hub = rt.block_on(async { PromptHub::new(&db_path, config).await.unwrap() });

    let identity = AgentIdentity::default();

    c.bench_function("insert_prompt", |b| {
        b.to_async(&rt).iter_batched(
            || Prompt {
                id: Uuid::new_v4(),
                name: format!("bench-{}", Uuid::new_v4()),
                version: semver::Version::new(0, 1, 0),
                status: Status::Active,
                system_prompt: "Be helpful.".to_string(),
                user_template: "{{input}}".to_string(),
                required_vars: vec!["input".to_string()],
                domain: Domain::General,
                tags: vec!["benchmark".to_string()],
                target_roles: vec![],
                metadata: PromptMeta::default(),
                metrics: PromptMetrics::default(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                author: AgentIdentity::default(),
                deleted_at: None,
                generation_params: None,
                locale: None,
                multimodal: None,
            },
            |prompt| async {
                let _ = hub.register(black_box(prompt), &identity).await;
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_search_fast(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("bench.db");

    let config = HubConfig::default();
    let hub = rt.block_on(async { PromptHub::new(&db_path, config).await.unwrap() });

    // Pre-populate with 100 prompts
    let identity = AgentIdentity::default();
    rt.block_on(async {
        for i in 0..100 {
            let prompt = Prompt {
                id: Uuid::new_v4(),
                name: format!("search-test-{i}"),
                version: semver::Version::new(0, 1, 0),
                status: Status::Active,
                system_prompt: format!("Help with {i}"),
                user_template: "{{input}}".to_string(),
                required_vars: vec!["input".to_string()],
                domain: Domain::General,
                tags: vec!["search".to_string()],
                target_roles: vec![],
                metadata: PromptMeta::default(),
                metrics: PromptMetrics::default(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                author: identity.clone(),
                deleted_at: None,
                generation_params: None,
                locale: None,
                multimodal: None,
            };
            let _ = hub.register(prompt, &identity).await;
        }
    });

    c.bench_function("search_fast_100_prompts", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = hub
                .search(
                    "test",
                    SearchMode::Fast,
                    SearchFilters::default(),
                    Pagination::default(),
                )
                .await;
        });
    });
}

fn bench_search_smart(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("bench_smart.db");

    let config = HubConfig::default();
    let hub = rt.block_on(async { PromptHub::new(&db_path, config).await.unwrap() });

    // Pre-populate with 50 prompts
    let identity = AgentIdentity::default();
    rt.block_on(async {
        for i in 0..50 {
            let prompt = Prompt {
                id: Uuid::new_v4(),
                name: format!("smart-test-{i}"),
                version: semver::Version::new(0, 1, 0),
                status: Status::Active,
                system_prompt: format!("Explain topic {i} in detail with examples"),
                user_template: "{{input}}".to_string(),
                required_vars: vec!["input".to_string()],
                domain: Domain::General,
                tags: vec!["smart".to_string()],
                target_roles: vec![],
                metadata: PromptMeta::default(),
                metrics: PromptMetrics::default(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                author: identity.clone(),
                deleted_at: None,
                generation_params: None,
                locale: None,
                multimodal: None,
            };
            let _ = hub.register(prompt, &identity).await;
        }
    });

    let mut group = c.benchmark_group("search_smart");
    group.bench_function("smart_50_prompts", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = hub
                .search(
                    "explain",
                    SearchMode::Smart,
                    SearchFilters::default(),
                    Pagination::default(),
                )
                .await;
        });
    });
    group.finish();
}

fn bench_lock_unlock(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("bench_lock.db");

    let config = HubConfig::default();
    let hub = rt.block_on(async { PromptHub::new(&db_path, config).await.unwrap() });

    let identity = AgentIdentity::default();
    let prompt_id = Uuid::new_v4();

    c.bench_function("lock_acquire", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = hub
                .lock(prompt_id, &identity, std::time::Duration::from_secs(60))
                .await;
        });
    });
}

criterion_group!(
    benches,
    bench_insert_prompt,
    bench_search_fast,
    bench_search_smart,
    bench_lock_unlock
);
criterion_main!(benches);
