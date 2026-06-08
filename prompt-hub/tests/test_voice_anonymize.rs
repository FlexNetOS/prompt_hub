#![forbid(unsafe_code)]

/// Integration test: default anonymizer detects and replaces email, phone, SSN.
#[test]
fn test_anonymizer_detects_common_pii() {
    use prompt_hub::voice_anonymize::Anonymizer;

    let anonymizer = Anonymizer::default_with_builtins();
    let transcript = "Call John at john@example.com or 555-123-4567. My SSN is 123-45-6789.";
    let (anonymized, matches) = anonymizer.anonymize(transcript).unwrap();

    // Should have detected at least email + phone + SSN = 3 PII instances.
    assert!(
        matches.len() >= 3,
        "Expected ≥3 PII matches, got {}",
        matches.len()
    );

    // Verify each original PII is replaced with a placeholder in anonymized text.
    for m in &matches {
        assert!(
            !anonymized.contains(&m.original),
            "PII '{}' should be replaced",
            m.original
        );
        assert!(
            anonymized.contains(&m.placeholder),
            "Placeholder '{}' not found in output",
            m.placeholder
        );
    }

    // Verify non-PII text is preserved.
    assert!(anonymized.contains("Call John at") || anonymized.contains("John"));
}

/// Integration test: empty text returns empty output with zero matches.
#[test]
fn test_anonymizer_empty_text() {
    use prompt_hub::voice_anonymize::Anonymizer;

    let anonymizer = Anonymizer::default_with_builtins();
    let (text, matches) = anonymizer.anonymize("").unwrap();

    assert!(text.is_empty());
    assert!(matches.is_empty());
}

/// Integration test: text with no PII passes through unchanged.
#[test]
fn test_anonymizer_no_pii_passthrough() {
    use prompt_hub::voice_anonymize::Anonymizer;

    let anonymizer = Anonymizer::default_with_builtins();
    let plain = "This is a normal sentence with no personally identifiable information.";
    let (result, matches) = anonymizer.anonymize(plain).unwrap();

    assert_eq!(result, plain);
    assert!(matches.is_empty());
}

/// Integration test: custom pattern can be added at runtime.
#[test]
fn test_anonymizer_custom_pattern() {
    use prompt_hub::voice_anonymize::{AnonymizerBuilder, PiiPattern};

    let pattern = PiiPattern::new(
        prompt_hub::voice_anonymize::PiiType::Custom("SECRET_CODE".into()),
        r"\bSECRET-[A-Z0-9]{4}\b",
        "[SECRET_CODE]",
    )
    .expect("valid pattern");

    let builder = AnonymizerBuilder::new();
    let builder = builder.add_pattern(pattern).expect("valid pattern");
    let anonymizer = builder.build().unwrap();

    let text = "Use code SECRET-AB12 for access.";
    let (result, matches) = anonymizer.anonymize(text).unwrap();

    assert!(!matches.is_empty(), "Expected custom pattern to match");
    assert!(result.contains("[SECRET_CODE]"));
    assert!(!result.contains("SECRET-AB12"));
}
