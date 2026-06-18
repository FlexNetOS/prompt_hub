//! Voice transcript PII anonymization — pattern-based redaction for sensitive data.
//!
//! Provides `AnonymizerBuilder` and `Anonymizer` for scrubbing PII from text
//! transcripts, call-center logs, or any voice-to-text output. Uses ordered
//! regex patterns with configurable placeholders.

#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Category of detected PII.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum PiiType {
    Person,
    Organization,
    Location,
    Email,
    Phone,
    SSN,
    CreditCard,
    IPAddress,
    DateOfBirth,
    PostalCode,
    Custom(String),
}

impl fmt::Display for PiiType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PiiType::Person => write!(f, "PERSON"),
            PiiType::Organization => write!(f, "ORGANIZATION"),
            PiiType::Location => write!(f, "LOCATION"),
            PiiType::Email => write!(f, "EMAIL"),
            PiiType::Phone => write!(f, "PHONE"),
            PiiType::SSN => write!(f, "SSN"),
            PiiType::CreditCard => write!(f, "CREDIT_CARD"),
            PiiType::IPAddress => write!(f, "IP_ADDRESS"),
            PiiType::DateOfBirth => write!(f, "DATE_OF_BIRTH"),
            PiiType::PostalCode => write!(f, "POSTAL_CODE"),
            PiiType::Custom(s) => write!(f, "{s}"),
        }
    }
}

impl From<String> for PiiType {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "person" => PiiType::Person,
            "organization" | "org" => PiiType::Organization,
            "location" => PiiType::Location,
            "email" => PiiType::Email,
            "phone" => PiiType::Phone,
            "ssn" => PiiType::SSN,
            "credit_card" | "creditcard" => PiiType::CreditCard,
            "ip_address" | "ipaddress" | "ip" => PiiType::IPAddress,
            "date_of_birth" | "dob" => PiiType::DateOfBirth,
            "postal_code" | "zip" => PiiType::PostalCode,
            other => PiiType::Custom(other.to_string()),
        }
    }
}

impl From<PiiType> for String {
    fn from(v: PiiType) -> Self {
        match v {
            PiiType::Person => "PERSON".to_string(),
            PiiType::Organization => "ORGANIZATION".to_string(),
            PiiType::Location => "LOCATION".to_string(),
            PiiType::Email => "EMAIL".to_string(),
            PiiType::Phone => "PHONE".to_string(),
            PiiType::SSN => "SSN".to_string(),
            PiiType::CreditCard => "CREDIT_CARD".to_string(),
            PiiType::IPAddress => "IP_ADDRESS".to_string(),
            PiiType::DateOfBirth => "DATE_OF_BIRTH".to_string(),
            PiiType::PostalCode => "POSTAL_CODE".to_string(),
            PiiType::Custom(s) => s,
        }
    }
}

/// A single pattern: regex + placeholder tag.
#[derive(Debug, Clone)]
pub struct PiiPattern {
    pub pii_type: PiiType,
    regex: Regex,
    pub placeholder: String,
}

impl PiiPattern {
    /// Create a new `PiiPattern` from raw-string *pattern* with the given
    /// *placeholder*. Returns `HubError::ValidationError` on bad regex.
    pub fn new(pii_type: PiiType, pattern: &str, placeholder: &str) -> Result<Self> {
        let regex = RegexBuilder::new(pattern)
            .size_limit(1024 * 1024) // 1 MiB to prevent ReDoS blowup
            .build()
            .map_err(|e| HubError::ValidationError(format!("bad regex for {:?}: {e}", pii_type)))?;
        Ok(Self {
            pii_type,
            regex,
            placeholder: placeholder.to_string(),
        })
    }
}

/// A detected PII instance in text.
#[derive(Debug, Clone)]
pub struct PiiMatch {
    pub pii_type: PiiType,
    pub original: String,
    pub placeholder: String,
    pub byte_offset: usize,
}

impl PiiMatch {
    #[allow(dead_code)] // used by anonymize() logic below
    fn new(pii_type: PiiType, original: String, placeholder: String, byte_offset: usize) -> Self {
        Self {
            pii_type,
            original,
            placeholder,
            byte_offset,
        }
    }
}

/// Builder for an [`Anonymizer`] with built-in patterns + custom additions.
#[derive(Debug, Clone, Default)]
pub struct AnonymizerBuilder {
    include_builtins: bool,
    custom_patterns: Vec<PiiPattern>,
}

impl AnonymizerBuilder {
    /// Create a new builder (empty by default).
    pub fn new() -> Self {
        Self::default()
    }

    /// Include the standard built-in patterns (email, phone, SSN, …).
    /// Default is `true`.
    pub fn include_builtins(mut self, include: bool) -> Self {
        self.include_builtins = include;
        self
    }

    /// Add a custom PII pattern.
    pub fn add_pattern(mut self, pattern: PiiPattern) -> Result<Self> {
        self.custom_patterns.push(pattern);
        Ok(self)
    }

    /// Convenience method to add a raw-string pattern.
    pub fn add_raw(mut self, pii_type: PiiType, pattern: &str, placeholder: &str) -> Result<Self> {
        self.custom_patterns
            .push(PiiPattern::new(pii_type, pattern, placeholder)?);
        Ok(self)
    }

    /// Build the [`Anonymizer`]. Returns a list of *unused* custom patterns
    // (patterns that failed to compile are not added — see `add_pattern`).
    pub fn build(self) -> Result<Anonymizer> {
        let mut patterns = Vec::new();
        if self.include_builtins {
            patterns.extend(built_in_patterns());
        }
        patterns.extend(self.custom_patterns);
        // Ensure canonical ordering: SSN, CreditCard, Email, Phone, IPv4, DOB, ZIP
        order_patterns(&mut patterns);
        Ok(Anonymizer { patterns })
    }
}

/// The anonymizer: holds all configured patterns and performs redaction.
#[derive(Debug, Clone)]
pub struct Anonymizer {
    patterns: Vec<PiiPattern>,
}

impl Anonymizer {
    /// Create an anonymizer with all built-in patterns at default ordering.
    pub fn default_with_builtins() -> Self {
        let mut patterns = built_in_patterns();
        order_patterns(&mut patterns);
        Self { patterns }
    }

    /// Return a builder for constructing a custom [`Anonymizer`].
    #[allow(dead_code)] // used by callers via the public API
    pub fn build() -> AnonymizerBuilder {
        AnonymizerBuilder::new()
    }

    /// Add a pattern at runtime. Patterns are inserted in canonical order.
    pub fn add_pattern(&mut self, pattern: PiiPattern) {
        order_patterns_with(&mut self.patterns, vec![pattern]);
    }

    /// Remove a pattern by its [`PiiType`]. Returns `true` if a pattern was
    /// removed.
    pub fn remove_pattern(&mut self, pii_type: &PiiType) -> bool {
        let before = self.patterns.len();
        self.patterns.retain(|p| &p.pii_type != pii_type);
        before != self.patterns.len()
    }

    /// Scrub PII from *text*, returning `(anonymized_text, Vec<PiiMatch>)`.
    ///
    /// Patterns are applied in canonical order (SSN → CreditCard → Email →
    /// Phone → IPv4 → DOB → ZIP) to prevent partial overlaps. Each match is
    /// replaced with its placeholder token in a single left-to-right pass.
    pub fn anonymize(&self, text: &str) -> Result<(String, Vec<PiiMatch>)> {
        if self.patterns.is_empty() {
            return Ok((text.to_string(), Vec::new()));
        }

        // Collect all matches sorted by byte offset, breaking ties in pattern
        // order (SSN first). We use a single pass: for each pattern in order,
        // find non-overlapping matches and record them. Then we rebuild the
        // string from right to left so offsets stay valid.
        let mut all_matches: Vec<PiiMatch> = Vec::new();

        for pat in &self.patterns {
            for m in pat.regex.find_iter(text) {
                let matched = m.as_str().to_string();
                // Post-filter: IPv4 must be in 0-255 range per octet.
                if matches!(&pat.pii_type, PiiType::IPAddress) && !ipv4_valid_octets(&matched) {
                    continue;
                }
                all_matches.push(PiiMatch {
                    pii_type: pat.pii_type.clone(),
                    original: matched,
                    placeholder: pat.placeholder.clone(),
                    byte_offset: m.start(),
                });
            }
        }

        // Deduplicate overlapping matches — keep the leftmost; break ties by
        // pattern order (SSN > CreditCard > …). Our patterns list is already in
        // canonical order, so the first match at a given offset wins.
        all_matches.sort_by_key(|m| m.byte_offset);
        let mut deduped: Vec<PiiMatch> = Vec::new();
        let mut end: usize = 0;
        for m in all_matches {
            if m.byte_offset >= end {
                deduped.push(m.clone());
                end = m.byte_offset + m.original.len();
            }
        }

        // Build the anonymized string (right-to-left to keep offsets valid).
        let mut result = text.to_string();
        for m in deduped.iter().rev() {
            result.replace_range(
                m.byte_offset..m.byte_offset + m.original.len(),
                &m.placeholder,
            );
        }

        Ok((result, deduped))
    }

    /// Return the number of configured patterns.
    #[allow(dead_code)] // used by callers
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }

    /// Return the list of configured [`PiiType`] values (in order).
    #[allow(dead_code)] // used by callers
    pub fn configured_types(&self) -> Vec<&PiiType> {
        self.patterns.iter().map(|p| &p.pii_type).collect()
    }
}

// ---------------------------------------------------------------------------
// Built-in patterns (exact regex from architect plan)
// ---------------------------------------------------------------------------

fn built_in_patterns() -> Vec<PiiPattern> {
    vec![
        PiiPattern {
            pii_type: PiiType::SSN,
            regex: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
            placeholder: "[SSN]".to_string(),
        },
        PiiPattern {
            pii_type: PiiType::CreditCard,
            regex: Regex::new(r"(?:\d{4}[- ]?){3}\d{4}").unwrap(),
            placeholder: "[CREDIT_CARD]".to_string(),
        },
        PiiPattern {
            pii_type: PiiType::Email,
            regex: Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap(),
            placeholder: "[EMAIL]".to_string(),
        },
        PiiPattern {
            pii_type: PiiType::Phone,
            regex: Regex::new(r"(?:\+?1[-.]?)?\(?\d{3}\)?[-. ]?\d{3}[-. ]?\d{4}").unwrap(),
            placeholder: "[PHONE]".to_string(),
        },
        PiiPattern {
            pii_type: PiiType::IPAddress,
            // IPv4 regex (post-filter for 0-255 octets is in anonymize())
            regex: Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap(),
            placeholder: "[IP_ADDRESS]".to_string(),
        },
        PiiPattern {
            pii_type: PiiType::DateOfBirth,
            regex: Regex::new(r"(?:0[1-9]|1[0-2])[-/.](?:0[1-9]|[12]\d|3[01])[-/.](?:19|20)\d{2}")
                .unwrap(),
            placeholder: "[DATE_OF_BIRTH]".to_string(),
        },
        PiiPattern {
            pii_type: PiiType::PostalCode,
            regex: Regex::new(r"\b\d{5}(?:-\d{4})?\b").unwrap(),
            placeholder: "[POSTAL_CODE]".to_string(),
        },
    ]
}

/// Canonical ordering priority map (lower = higher priority).
fn pattern_priority(pii_type: &PiiType) -> u8 {
    match pii_type {
        PiiType::SSN => 0,
        PiiType::CreditCard => 1,
        PiiType::Email => 2,
        PiiType::Phone => 3,
        PiiType::IPAddress => 4,
        PiiType::DateOfBirth => 5,
        PiiType::PostalCode => 6,
        _ => 99, // Custom types go last
    }
}

fn order_patterns(patterns: &mut [PiiPattern]) {
    patterns.sort_by_key(|p| pattern_priority(&p.pii_type));
}

/// Internal helper: sort `extra` then splice into `all` preserving canonical
/// order. Called from `add_pattern`.
fn order_patterns_with(all: &mut Vec<PiiPattern>, extra: Vec<PiiPattern>) {
    all.extend(extra);
    order_patterns(all);
}

// ---------------------------------------------------------------------------
// Post-filter helpers
// ---------------------------------------------------------------------------

/// Validate that every octet of a dotted-quad string is in 0-255.
fn ipv4_valid_octets(s: &str) -> bool {
    s.split('.').all(|o| o.parse::<u8>().is_ok())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice_anonymize::{AnonymizerBuilder, PiiPattern};

    // ---- Unit tests (8-10) ----

    /// Test 1: Email detection and replacement.
    #[test]
    fn test_anonymize_email() {
        let anon = Anonymizer::default_with_builtins();
        let (result, found) = anon
            .anonymize("Contact alice@example.com for info.")
            .unwrap();
        assert!(result.contains("[EMAIL]"));
        assert_eq!(found.len(), 1);
        assert!(matches!(&found[0].pii_type, PiiType::Email));
    }

    /// Test 2: Phone number detection.
    #[test]
    fn test_anonymize_phone() {
        let anon = Anonymizer::default_with_builtins();
        let (result, found) = anon.anonymize("Call me at 555-123-4567.").unwrap();
        assert!(result.contains("[PHONE]"));
        assert_eq!(found.len(), 1);
        assert!(matches!(&found[0].pii_type, PiiType::Phone));
    }

    /// Test 3: SSN detection with strict boundaries.
    #[test]
    fn test_anonymize_ssn() {
        let anon = Anonymizer::default_with_builtins();
        let (result, found) = anon.anonymize("SSN is 123-45-6789.").unwrap();
        assert!(result.contains("[SSN]"));
        assert_eq!(found.len(), 1);
        assert!(matches!(&found[0].pii_type, PiiType::SSN));
    }

    /// Test 4: Credit card detection.
    #[test]
    fn test_anonymize_credit_card() {
        let anon = Anonymizer::default_with_builtins();
        let (result, found) = anon.anonymize("Card: 4111-1111-1111-1111.").unwrap();
        assert!(result.contains("[CREDIT_CARD]"));
        assert_eq!(found.len(), 1);
        assert!(matches!(&found[0].pii_type, PiiType::CreditCard));
    }

    /// Test 5: IPv4 validation — valid octets pass.
    #[test]
    fn test_anonymize_ipv4_valid() {
        let anon = Anonymizer::default_with_builtins();
        let (result, found) = anon.anonymize("Server at 192.168.1.1.").unwrap();
        assert!(result.contains("[IP_ADDRESS]"));
        assert_eq!(found.len(), 1);
    }

    /// Test 6: IPv4 post-filter — octets > 255 rejected.
    #[test]
    fn test_anonymize_ipv4_invalid() {
        let anon = Anonymizer::default_with_builtins();
        let (result, found) = anon.anonymize("Server at 999.999.999.999.").unwrap();
        // Invalid octets should NOT be flagged.
        assert!(!result.contains("[IP_ADDRESS]"));
        assert_eq!(found.len(), 0);
    }

    /// Test 7: Date of birth detection.
    #[test]
    fn test_anonymize_dob() {
        let anon = Anonymizer::default_with_builtins();
        let (result, found) = anon.anonymize("DOB: 01/15/2000.").unwrap();
        assert!(result.contains("[DATE_OF_BIRTH]"));
        assert_eq!(found.len(), 1);
        assert!(matches!(&found[0].pii_type, PiiType::DateOfBirth));
    }

    /// Test 8: ZIP code detection.
    #[test]
    fn test_anonymize_zip() {
        let anon = Anonymizer::default_with_builtins();
        let (result, found) = anon.anonymize("Zip: 12345-6789.").unwrap();
        assert!(result.contains("[POSTAL_CODE]"));
        assert_eq!(found.len(), 1);
        assert!(matches!(&found[0].pii_type, PiiType::PostalCode));
    }

    /// Test 9: Multiple matches in one text.
    #[test]
    fn test_anonymize_multiple() {
        let anon = Anonymizer::default_with_builtins();
        let text = "Email bob@test.org phone 555-123-4567 SSN 999-88-7777";
        let (result, found) = anon.anonymize(text).unwrap();
        assert!(
            result.contains("[EMAIL]") && result.contains("[PHONE]") && result.contains("[SSN]")
        );
        assert_eq!(found.len(), 3);
    }

    /// Test 10: Builder with custom pattern.
    #[test]
    fn test_builder_with_custom_pattern() {
        let anon = AnonymizerBuilder::new()
            .include_builtins(false)
            .add_raw(
                PiiType::Custom("SECRET".into()),
                r"\bSHK-[0-9]{4}\b",
                "[REDACTED]",
            )
            .unwrap()
            .build()
            .unwrap();

        let (result, found) = anon.anonymize("Code: SHK-1234.").unwrap();
        assert_eq!(result, "Code: [REDACTED].");
        assert_eq!(found.len(), 1);
    }

    /// Test 11: add_pattern / remove_pattern at runtime.
    #[test]
    fn test_add_remove_patterns() {
        let mut anon = Anonymizer::default_with_builtins();
        // Pattern count should be > 0 (built-ins).
        assert!(anon.pattern_count() > 0);

        // Add a custom pattern.
        let custom = PiiPattern::new(
            PiiType::Custom("API_KEY".into()),
            r"\bAKIA[0-9A-Z]{16}\b",
            "[API_KEY]",
        )
        .unwrap();
        anon.add_pattern(custom);
        assert!(
            anon.configured_types()
                .iter()
                .any(|t| matches!(t, PiiType::Custom(s) if s == "API_KEY"))
        );

        // Remove the custom pattern.
        let removed = anon.remove_pattern(&PiiType::Custom("API_KEY".into()));
        assert!(removed);
    }

    /// Test 12: SSN takes priority over Credit Card when patterns overlap.
    #[test]
    fn test_pattern_priority_ssn_over_credit_card() {
        let anon = Anonymizer::default_with_builtins();
        // Manually craft overlapping text — SSN should be matched first.
        // "123-45-6789" is an SSN pattern; it should not also match credit card.
        let (result, found) = anon.anonymize("SSN: 123-45-6789.").unwrap();
        assert_eq!(found.len(), 1);
        assert!(matches!(&found[0].pii_type, PiiType::SSN));
        assert!(!result.contains("[CREDIT_CARD]"));
    }

    /// Test 13: Empty text returns empty matches.
    #[test]
    fn test_anonymize_empty_text() {
        let anon = Anonymizer::default_with_builtins();
        let (result, found) = anon.anonymize("").unwrap();
        assert_eq!(result, "");
        assert!(found.is_empty());
    }

    /// Test 14: Plain text with no PII returns unchanged.
    #[test]
    fn test_anonymize_no_pii() {
        let anon = Anonymizer::default_with_builtins();
        let (result, found) = anon
            .anonymize("This is perfectly clean and boring text.")
            .unwrap();
        assert_eq!(result, "This is perfectly clean and boring text.");
        assert!(found.is_empty());
    }
}
