#![forbid(unsafe_code)]

//! Integration tests for the accessibility feature.
//!
//! Feature-gated: requires `accessibility` feature flag on prompt-hub.

#[cfg(feature = "accessibility")]
mod integration {
    use prompt_hub::accessibility::{AccessibilityConfig, AccessibleOutput, transform};
    use prompt_hub::{HubConfig, PromptHub};

    /// Hub integration: accessible_output with PlainText format.
    #[tokio::test]
    async fn test_accessible_output_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        let content = "Hello   world\n\nThis is a test.";
        let config = AccessibilityConfig::plain();
        let output = hub.accessible_output(content, config).await.unwrap();

        match output {
            AccessibleOutput::Plain(text) => {
                assert!(text.contains("Hello world"));
                assert!(!text.contains("   ")); // excess whitespace stripped
            }
            _ => panic!("Expected Plain output"),
        }
    }

    /// Hub integration: accessible_output_all returns multi-sensory output.
    #[tokio::test]
    async fn test_accessible_output_all() {
        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        let content = "Test content with * item and ** item";
        let output = hub.accessible_output_all(content).await.unwrap();

        // All three formats should be present and non-empty
        assert!(matches!(&output.plain, AccessibleOutput::Plain(_)));
        assert!(matches!(
            &output.structured,
            AccessibleOutput::Structured(_)
        ));
        assert!(matches!(&output.braille, AccessibleOutput::Braille(_)));

        match &output.plain {
            AccessibleOutput::Plain(t) => assert!(!t.is_empty()),
            _ => panic!("plain should be Plain"),
        }
        match &output.structured {
            AccessibleOutput::Structured(v) => assert!(v.get("sections").is_some()),
            _ => panic!("structured should be Structured"),
        }
        match &output.braille {
            AccessibleOutput::Braille(t) => assert!(!t.is_empty()),
            _ => panic!("braille should be Braille"),
        }
    }

    /// Edge case: empty content returns error.
    #[tokio::test]
    async fn test_empty_content_errors() {
        let dir = tempfile::tempdir().unwrap();
        let hub = PromptHub::new(&dir.path().join("prompthub.db"), HubConfig::default())
            .await
            .unwrap();

        let config = AccessibilityConfig::plain();
        let result = hub.accessible_output("", config).await;
        assert!(result.is_err());
    }

    /// Unit test: plain_text normalizes whitespace in paragraphs.
    #[test]
    fn test_plain_text_normalizes_whitespace() {
        let input = "Hello   world\n\nThis   is  a paragraph.";
        let config = AccessibilityConfig::plain();
        let output = transform(input, &config).unwrap();

        match output {
            AccessibleOutput::Plain(text) => {
                assert!(text.contains("Hello world"));
                assert!(!text.contains("   "));
            }
            _ => panic!("Expected Plain"),
        }
    }

    /// Unit test: structured JSON detects lists and headings.
    #[test]
    fn test_structured_detects_content_types() {
        let input = "# Title\n- Item 1\n- Item 2\n```\ncode here\n```";
        let config = AccessibilityConfig::structured();
        let output = transform(input, &config).unwrap();

        match output {
            AccessibleOutput::Structured(json) => {
                assert!(json["sections"].is_array());
                let sections = json["sections"].as_array().unwrap();
                assert!(!sections.is_empty());
                assert!(json["metadata"]["has_lists"].as_bool().unwrap_or(false));

                // Check for at least one heading
                let has_heading = sections.iter().any(|s| s["type"] == "heading");
                assert!(has_heading, "Expected a heading section");
            }
            _ => panic!("Expected Structured"),
        }
    }

    /// Unit test: braille mapping for lowercase 'a' and uppercase 'A'.
    #[test]
    fn test_braille_correct_mapping() {
        let config = AccessibilityConfig::braille();
        let out_a = transform("a", &config).unwrap();
        let out_a_upper = transform("A", &config).unwrap();

        match (out_a, out_a_upper) {
            (AccessibleOutput::Braille(a_str), AccessibleOutput::Braille(a2_str)) => {
                assert_eq!(a_str.chars().next(), Some('\u{2801}'));
                assert_eq!(a2_str.chars().next(), Some('\u{2841}'));
            }
            _ => panic!("Expected Braille outputs"),
        }
    }

    /// Unit test: dyslexia-friendly adds middot separators.
    #[test]
    fn test_dyslexia_friendly_separators() {
        let input = "hello world foo";
        let config = AccessibilityConfig::dyslexia_friendly();
        let output = transform(input, &config).unwrap();

        match output {
            AccessibleOutput::Plain(text) => {
                assert!(text.contains('\u{00B7}'));
            }
            _ => panic!("Expected Plain"),
        }
    }

    /// Unit test: multisensory structured JSON includes plain_text and braille.
    #[test]
    fn test_multisensory_includes_all() {
        let mut config = AccessibilityConfig::structured();
        config.multisensory = true;
        let output = transform("Test content", &config).unwrap();

        match output {
            AccessibleOutput::Structured(json) => {
                assert!(json.get("plain_text").is_some());
                assert!(json.get("braille").is_some());
                let pt = json["plain_text"]
                    .as_str()
                    .expect("plain_text should be string");
                let br = json["braille"].as_str().expect("braille should be string");
                assert!(!pt.is_empty());
                assert!(!br.is_empty());
            }
            _ => panic!("Expected Structured"),
        }
    }
}
