use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use prompt_hub::search::SmartEngine;
use prompt_hub::storage::{Storage, StorageConfig};
use std::sync::Arc;

fn bench_mock_embed(c: &mut Criterion) {
    // Create an in-memory storage for the benchmark
    let rt = tokio::runtime::Runtime::new().unwrap();
    let storage = rt.block_on(async {
        let config = StorageConfig {
            db_path: ":memory:".to_string(),
            max_connections: 2,
            wal_mode: false,
            foreign_keys: true,
        };
        Arc::new(
            Storage::new(config)
                .await
                .expect("Failed to create in-memory storage"),
        )
    });

    let engine = SmartEngine::new("all-MiniLM-L6-v2", storage);

    let inputs = vec![
        ("short", "Sort a list"),
        (
            "medium",
            "Build a React login page with Google OAuth, dark mode, and Tailwind CSS using Next.js 14",
        ),
        (
            "long",
            "Create a microservices architecture with a GraphQL API gateway, gRPC inter-service communication, PostgreSQL primary database, Redis caching layer, Kafka event streaming, Kubernetes deployment manifests, Prometheus monitoring, and Jaeger distributed tracing. Include CI/CD pipelines with GitHub Actions, Terraform infrastructure as code, and comprehensive integration tests.",
        ),
    ];

    let mut group = c.benchmark_group("mock_embed");
    for (name, text) in &inputs {
        group.bench_with_input(BenchmarkId::new("length", name), *text, |b, text| {
            b.iter(|| engine.mock_embed(black_box(text)));
        });
    }
    group.finish();
}

fn bench_cosine_similarity(c: &mut Criterion) {
    let a: Vec<f32> = (0..384).map(|i| (i as f32) / 384.0).collect();
    let b: Vec<f32> = (0..384).map(|i| ((i + 1) as f32) / 384.0).collect();

    c.bench_function("cosine_similarity_384d", |b| {
        b.iter(|| SmartEngine::cosine_similarity(black_box(&a), black_box(&b)));
    });
}

fn bench_mock_embed_consistency(c: &mut Criterion) {
    // Benchmark verifying mock_embed produces deterministic results
    let rt = tokio::runtime::Runtime::new().unwrap();
    let storage = rt.block_on(async {
        let config = StorageConfig {
            db_path: ":memory:".to_string(),
            max_connections: 2,
            wal_mode: false,
            foreign_keys: true,
        };
        Arc::new(
            Storage::new(config)
                .await
                .expect("Failed to create in-memory storage"),
        )
    });

    let engine = SmartEngine::new("all-MiniLM-L6-v2", storage);

    let mut group = c.benchmark_group("mock_embed_consistency");
    group.bench_function("embed_then_cosine", |b| {
        b.iter(|| {
            let v1 = engine.mock_embed(black_box("benchmark query text"));
            let v2 = engine.mock_embed(black_box("benchmark query text"));
            let sim = SmartEngine::cosine_similarity(&v1, &v2);
            black_box(sim);
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_mock_embed,
    bench_cosine_similarity,
    bench_mock_embed_consistency
);
criterion_main!(benches);
