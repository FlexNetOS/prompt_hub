#![forbid(unsafe_code)]

use prompt_hub::chaos::{ChaosConfig, ChaosEngine, ChaosResult, ChaosSeverity, ChaosStrategy};

/// Integration test: verify the chaos feature gates compile and that
/// ChaosEngine can be instantiated, run with a mock executor, and produce results.
#[cfg(feature = "chaos")]
#[tokio::test]
async fn test_chaos_engine_produces_results() {
    let engine = ChaosEngine::new();

    let config = ChaosConfig {
        target_prompt_id: uuid::Uuid::new_v4(),
        strategies: vec![
            prompt_hub::chaos::ChaosStrategy::TextMutation(
                prompt_hub::chaos::TextMutationConfig::default(),
            ),
            prompt_hub::chaos::ChaosStrategy::AdversarialSuffix,
        ],
        iterations_per_strategy: 10,
        failure_threshold: 0.95,
        max_output_tokens: 2048,
        seed: Some(42),
    };

    // Mock executor that always returns valid output
    let results = engine
        .run(config, |_prompt| async {
            "Valid response to prompt".to_string()
        })
        .await;

    assert_eq!(results.len(), 2);
    for result in &results {
        assert_eq!(result.total_tests, 10);
        assert_eq!(result.failed_tests, 0);
        assert_eq!(result.pass_rate, 1.0);
    }
}

/// Integration test: verify ChaosSeverity classification is correct.
#[test]
fn test_severity_classification() {
    // Helper to construct a ChaosResult and check severity via the public field
    let sev = |pass_rate: f64| -> ChaosSeverity {
        let r = ChaosResult {
            prompt_id: uuid::Uuid::nil(),
            strategy: ChaosStrategy::AdversarialSuffix,
            pass_rate: pass_rate.clamp(0.0, 1.0),
            total_tests: 100,
            failed_tests: ((1.0 - pass_rate) * 100.0) as u32,
            severity: ChaosResult::severity_for(pass_rate),
        };
        r.severity
    };

    // pass_rate >= 0.95 => Resilient
    assert!(matches!(sev(1.0), ChaosSeverity::Resilient));
    assert!(matches!(sev(0.95), ChaosSeverity::Resilient));
    assert!(matches!(sev(0.96), ChaosSeverity::Resilient));

    // pass_rate between 0.5 and 0.95 => Vulnerable
    assert!(matches!(sev(0.80), ChaosSeverity::Vulnerable));
    assert!(matches!(sev(0.70), ChaosSeverity::Vulnerable));

    // pass_rate < 0.5 => Fragile
    assert!(matches!(sev(0.30), ChaosSeverity::Fragile));

    // Edge: exactly 0.5 => Fragile (strictly less than 0.5 threshold)
    assert!(matches!(sev(0.50), ChaosSeverity::Fragile));
}

/// Integration test: verify assess_validity across boundary cases.
#[test]
fn test_assess_validity_boundaries() {
    // Valid outputs
    assert!(ChaosEngine::assess_validity("x", "hello"));
    assert!(ChaosEngine::assess_validity("x", "no problem at all"));

    // The word "error" embedded in other words should also be caught
    // (this is by design — we check for substrings, not whole words)
    assert!(!ChaosEngine::assess_validity(
        "x",
        "This has an error inside"
    ));

    // Whitespace-only is invalid
    assert!(!ChaosEngine::assess_validity("x", "   \t\n  "));

    // Empty string is invalid
    assert!(!ChaosEngine::assess_validity("x", ""));

    // Valid non-ASCII output
    assert!(ChaosEngine::assess_validity("x", "こんにちは"));

    // Contains "cannot" — should be caught even as substring
    assert!(!ChaosEngine::assess_validity(
        "x",
        "It cannot process the request"
    ));
}

/// Integration test: verify ChaosConfig defaults.
#[test]
fn test_chaos_config_defaults() {
    let config = ChaosConfig::default();

    assert_eq!(config.iterations_per_strategy, 50);
    assert_eq!(config.failure_threshold, 0.95);
    assert_eq!(config.max_output_tokens, 2048);
    assert!(config.seed.is_none());
    assert!(config.strategies.is_empty());
}
