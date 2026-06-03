#![forbid(unsafe_code)]

use crate::error::Result;
use crate::models::*;
use regex::Regex;
use std::sync::LazyLock;
use tracing::{info, instrument, warn};

/// Privacy scanner for detecting secrets and PII in user input.
///
/// Composed of a `SecretDetector` for API keys, tokens, and credentials,
/// and a `PiiDetector` for emails, phone numbers, and SSNs.
#[derive(Debug, Clone, Default)]
pub struct PrivacyScanner {
    pub secret_detector: SecretDetector,
    pub pii_detector: PiiDetector,
}

/// Detector for secrets (API keys, tokens, passwords).
#[derive(Debug, Clone, Default)]
pub struct SecretDetector;

/// Detector for personally identifiable information (PII).
#[derive(Debug, Clone, Default)]
pub struct PiiDetector;

impl PrivacyScanner {
    /// Scan user input for both secrets and PII.
    ///
    /// Returns a `PrivacyReport` containing all found issues.
    #[instrument]
    pub async fn scan(&self, input: &UserInput) -> Result<PrivacyReport> {
        let text = &input.extracted_text;

        info!("Starting privacy scan");

        let secret_issues = self.secret_detector.scan(text).await?;
        let pii_issues = self.pii_detector.scan(text).await?;

        let secrets_found = secret_issues.len();
        let pii_found = pii_issues.len();

        let mut issues = Vec::new();
        issues.extend(secret_issues);
        issues.extend(pii_issues);

        if secrets_found > 0 || pii_found > 0 {
            warn!(
                secrets = %secrets_found,
                pii = %pii_found,
                "Privacy issues detected"
            );
        } else {
            info!("No privacy issues found");
        }

        let risk_level = if secrets_found > 0 {
            "high"
        } else if pii_found > 0 {
            "medium"
        } else {
            "low"
        }
        .to_string();

        Ok(PrivacyReport {
            issues,
            sanitized: false,
            secrets_found,
            pii_found,
            risk_level,
        })
    }

    /// Redact sensitive information from text.
    ///
    /// Replaces API keys, passwords, secrets, and email addresses with
    /// `[REDACTED]` markers.
    pub fn redact(text: &str) -> String {
        static PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
            vec![
                (
                    Regex::new(r#"(?i)(api[_-]?key\s*[:=]\s*)['"]?[a-zA-Z0-9_\-]{20,}['"]?"#)
                        .unwrap(),
                    "${1}[REDACTED]",
                ),
                (
                    Regex::new(r#"(?i)(password\s*[:=]\s*)['"]?[^\s'"]+['"]?"#).unwrap(),
                    "${1}[REDACTED]",
                ),
                (
                    Regex::new(r#"(?i)(secret\s*[:=]\s*)['"]?[a-zA-Z0-9_\-]{10,}['"]?"#).unwrap(),
                    "${1}[REDACTED]",
                ),
                (
                    Regex::new(r#"(?i)(token\s*[:=]\s*)['"]?[a-zA-Z0-9_\-\.]{10,}['"]?"#).unwrap(),
                    "${1}[REDACTED]",
                ),
                (
                    Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap(),
                    "[EMAIL_REDACTED]",
                ),
            ]
        });

        let mut result = text.to_string();
        for (regex, replacement) in PATTERNS.iter() {
            result = regex.replace_all(&result, *replacement).to_string();
        }
        result
    }

    /// Create a new scanner with default detectors.
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretDetector {
    /// Scan text for secrets.
    ///
    /// Detects API keys, AWS credentials, GitHub tokens, and OpenAI API keys.
    pub async fn scan(&self, text: &str) -> Result<Vec<PrivacyIssue>> {
        let mut issues = Vec::new();

        static SECRET_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
            vec![
                (
                    Regex::new(r#"(?i)(api[_-]?key|apikey)\s*[:=]\s*['"]?[a-zA-Z0-9_\-]{16,}"#).unwrap(),
                    "api_key",
                ),
                (
                    Regex::new(r#"(?i)(aws_access_key_id|aws_secret_access_key)\s*[:=]\s*['"]?[A-Z0-9]{20}"#).unwrap(),
                    "aws_credential",
                ),
                (
                    Regex::new(r"ghp_[a-zA-Z0-9]{36}").unwrap(),
                    "github_pat",
                ),
                (
                    Regex::new(r"gho_[a-zA-Z0-9]{36}").unwrap(),
                    "github_oauth",
                ),
                (
                    Regex::new(r"sk-[a-zA-Z0-9]{20,}").unwrap(),
                    "openai_api_key",
                ),
                (
                    Regex::new(r"(?i)bearer\s+[a-zA-Z0-9_\-\.]{20,}").unwrap(),
                    "bearer_token",
                ),
                (
                    Regex::new(r#"(?i)private[_-]?key\s*[:=]\s*['"]?[a-zA-Z0-9+/=]{20,}"#).unwrap(),
                    "private_key",
                ),
            ]
        });

        for (pattern, secret_type) in SECRET_PATTERNS.iter() {
            for mat in pattern.find_iter(text) {
                issues.push(PrivacyIssue::Secret {
                    key: format!("{} at position {}", secret_type, mat.start()),
                });
            }
        }

        Ok(issues)
    }
}

impl PiiDetector {
    /// Scan text for personally identifiable information.
    ///
    /// Detects email addresses, phone numbers, and Social Security Numbers.
    pub async fn scan(&self, text: &str) -> Result<Vec<PrivacyIssue>> {
        let mut issues = Vec::new();

        // Email
        static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap()
        });
        for mat in EMAIL_RE.find_iter(text) {
            issues.push(PrivacyIssue::Pii {
                type_: "email".to_string(),
            });
            info!(position = %mat.start(), "Detected email PII");
        }

        // Phone (US format: 123-456-7890, 123.456.7890, 1234567890)
        static PHONE_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b").unwrap());
        for mat in PHONE_RE.find_iter(text) {
            issues.push(PrivacyIssue::Pii {
                type_: "phone".to_string(),
            });
            info!(position = %mat.start(), "Detected phone PII");
        }

        // SSN (US format: 123-45-6789)
        static SSN_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap());
        for mat in SSN_RE.find_iter(text) {
            issues.push(PrivacyIssue::Pii {
                type_: "ssn".to_string(),
            });
            warn!(position = %mat.start(), "Detected SSN PII");
        }

        // Credit card (simplified pattern: 16 digits in groups of 4)
        static CC_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b").unwrap());
        for mat in CC_RE.find_iter(text) {
            issues.push(PrivacyIssue::Pii {
                type_: "credit_card".to_string(),
            });
            warn!(position = %mat.start(), "Detected credit card PII");
        }

        Ok(issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_secret_detection_api_key() {
        let scanner = PrivacyScanner::default();
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "API_KEY=sk-test1234567890abcdef".to_string(),
        };
        let report = scanner.scan(&input).await.unwrap();
        assert!(report.secrets_found > 0, "Should detect API key");
        assert!(!report.issues.is_empty());
    }

    #[tokio::test]
    async fn test_secret_detection_github_token() {
        let scanner = PrivacyScanner::default();
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "Token is ghp_abcdefghijklmnopqrstuvwxyz0123456789AB".to_string(),
        };
        let report = scanner.scan(&input).await.unwrap();
        assert!(report.secrets_found > 0, "Should detect GitHub token");
    }

    #[tokio::test]
    async fn test_secret_detection_openai_key() {
        let scanner = PrivacyScanner::default();
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "sk-abcdefghijklmnopqrstuvwxyz12345678901234567890".to_string(),
        };
        let report = scanner.scan(&input).await.unwrap();
        assert!(report.secrets_found > 0, "Should detect OpenAI-style key");
    }

    #[tokio::test]
    async fn test_pii_detection_email() {
        let scanner = PrivacyScanner::default();
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "Contact me at john.doe@example.com please.".to_string(),
        };
        let report = scanner.scan(&input).await.unwrap();
        assert!(report.pii_found > 0, "Should detect email");
    }

    #[tokio::test]
    async fn test_pii_detection_phone() {
        let scanner = PrivacyScanner::default();
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "Call me at 555-123-4567.".to_string(),
        };
        let report = scanner.scan(&input).await.unwrap();
        assert!(report.pii_found > 0, "Should detect phone number");
    }

    #[tokio::test]
    async fn test_pii_detection_ssn() {
        let scanner = PrivacyScanner::default();
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "My SSN is 123-45-6789.".to_string(),
        };
        let report = scanner.scan(&input).await.unwrap();
        assert!(report.pii_found > 0, "Should detect SSN");
    }

    #[tokio::test]
    async fn test_no_issues_clean_text() {
        let scanner = PrivacyScanner::default();
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "Hello, this is a completely safe message with no secrets.".to_string(),
        };
        let report = scanner.scan(&input).await.unwrap();
        assert_eq!(report.secrets_found, 0);
        assert_eq!(report.pii_found, 0);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn test_redaction_api_key() {
        let text = "API_KEY=secret1234567890abcdef";
        let redacted = PrivacyScanner::redact(text);
        assert!(!redacted.contains("secret1234567890abcdef"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn test_redaction_email() {
        let text = "email: user@example.com";
        let redacted = PrivacyScanner::redact(text);
        assert!(!redacted.contains("user@example.com"));
        assert!(redacted.contains("[EMAIL_REDACTED]"));
    }

    #[test]
    fn test_redaction_password() {
        let text = "password=hunter2";
        let redacted = PrivacyScanner::redact(text);
        assert!(!redacted.contains("hunter2"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn test_redaction_multiple_issues() {
        let text = "API_KEY=secret1234567890abcdef and email: user@example.com";
        let redacted = PrivacyScanner::redact(text);
        assert!(!redacted.contains("secret1234567890abcdef"));
        assert!(!redacted.contains("user@example.com"));
    }

    #[test]
    fn test_redaction_no_issues() {
        let text = "Hello world, nothing sensitive here.";
        let redacted = PrivacyScanner::redact(text);
        assert_eq!(redacted, text);
    }

    #[tokio::test]
    async fn test_secret_detection_aws_credentials() {
        let scanner = PrivacyScanner::default();
        let input = UserInput {
            input_type: InputType::Text,
            raw_data: vec![],
            extracted_text: "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE".to_string(),
        };
        let report = scanner.scan(&input).await.unwrap();
        assert!(report.secrets_found > 0, "Should detect AWS credential");
    }

    #[tokio::test]
    async fn test_bearer_token_detection() {
        let detector = SecretDetector;
        let issues = detector
            .scan("Authorization: bearer abcdefghijklmnopqrstuvwxyz1234567890")
            .await
            .unwrap();
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, PrivacyIssue::Secret { key } if key.contains("bearer"))),
            "Should detect bearer token"
        );
    }
}
