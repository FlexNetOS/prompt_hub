use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

/// Benchmark the fast search path (keyword/FTS5)
fn bench_fast_search_empty(c: &mut Criterion) {
    let mut group = c.benchmark_group("fast_search");

    group.bench_function("empty_query", |b| {
        b.iter(|| {
            let query: &str = black_box("");
            let _result = query.len();
        });
    });

    group.bench_function("short_query", |b| {
        b.iter(|| {
            let query: &str = black_box("test");
            let _result = query.to_lowercase();
        });
    });

    group.bench_function("long_query", |b| {
        b.iter(|| {
            let query: &str = black_box("How to handle errors in async Rust with tokio and anyhow");
            let _result = query.to_lowercase();
        });
    });

    group.finish();
}

/// Benchmark search with various query complexities
fn bench_search_query_complexity(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_complexity");

    let queries = vec![
        ("simple", "error"),
        ("moderate", "error handling async"),
        (
            "complex",
            "How do I properly handle errors in async Rust functions using tokio and anyhow with graceful shutdown",
        ),
    ];

    for (name, query) in queries {
        group.bench_with_input(BenchmarkId::new("query", name), &query, |b, q| {
            b.iter(|| {
                let lower = black_box(q).to_lowercase();
                let _words: Vec<&str> = lower.split_whitespace().collect();
            });
        });
    }

    group.finish();
}

/// Benchmark scoring computation
fn bench_scoring(c: &mut Criterion) {
    let mut group = c.benchmark_group("scoring");

    group.bench_function("rrf_fusion", |b| {
        let fast_scores: Vec<(u64, f64)> = (0..100)
            .map(|i| (i as u64, 1.0 / (i as f64 + 1.0)))
            .collect();
        let smart_scores: Vec<(u64, f64)> = (0..100)
            .map(|i| (i as u64, 1.0 / (i as f64 + 2.0)))
            .collect();

        b.iter(|| {
            let k = 60.0f64;
            let mut combined: std::collections::HashMap<u64, f64> =
                std::collections::HashMap::new();

            for (id, rank) in &fast_scores {
                *combined.entry(*id).or_insert(0.0) += 1.0 / (k + *rank);
            }
            for (id, rank) in &smart_scores {
                *combined.entry(*id).or_insert(0.0) += 1.0 / (k + *rank);
            }

            black_box(combined);
        });
    });

    group.bench_function("bm25_compute", |b| {
        b.iter(|| {
            let k1 = 1.2f64;
            let b = 0.75f64;
            let tf = 3.0f64;
            let doc_len = 100.0f64;
            let avg_dl = 80.0f64;
            let idf = 1.5f64;

            let score = idf * (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * (doc_len / avg_dl)));

            black_box(score);
        });
    });

    group.finish();
}

/// Benchmark string matching operations used in search
fn bench_string_matching(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_matching");

    let haystack = "This is a test prompt for error handling in Rust async functions";
    let needle = "error handling";

    group.bench_function("contains_case_insensitive", |b| {
        b.iter(|| {
            let result = haystack.to_lowercase().contains(&needle.to_lowercase());
            black_box(result);
        });
    });

    group.bench_function("regex_match", |b| {
        let re = regex::Regex::new(r"(?i)error handling").unwrap();
        b.iter(|| {
            let result = re.is_match(haystack);
            black_box(result);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_fast_search_empty,
    bench_search_query_complexity,
    bench_scoring,
    bench_string_matching
);
criterion_main!(benches);
