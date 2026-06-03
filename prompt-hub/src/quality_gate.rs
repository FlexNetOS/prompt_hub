#![forbid(unsafe_code)]

use crate::error::Result;
use crate::models::*;
use std::future::Future;
use std::pin::Pin;
use tracing::{info, instrument};

/// Boxed future returned by object-safe checker traits.
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Best-effort human label for an artifact, used for logging.
///
/// `Artifact` is an enum without a stable id, so we derive a label from
/// whichever identifying field the variant carries.
fn artifact_label(artifact: &Artifact) -> &str {
    match artifact {
        Artifact::Prompt { .. } => "prompt",
        Artifact::Code { path, .. } => path,
        Artifact::Config { path, .. } => path,
        Artifact::Test { path, .. } => path,
        Artifact::Migration { path, .. } => path,
        Artifact::Documentation { title, .. } => title,
    }
}

/// Trait for linting artifacts.
///
/// Implementors check style, formatting, and structural correctness.
///
/// The method returns a boxed future so the trait stays usable as
/// `Box<dyn Linter>` (object-safe).
pub trait Linter: Send + Sync {
    /// Lint an artifact and return a result.
    fn lint<'a>(&'a self, artifact: &'a Artifact) -> BoxFuture<'a, Result<LintResult>>;
}

/// Trait for security scanning.
///
/// Implementors check for vulnerabilities, secrets, and unsafe patterns.
pub trait SecurityScanner: Send + Sync {
    /// Scan an artifact for security issues.
    fn scan<'a>(&'a self, artifact: &'a Artifact) -> BoxFuture<'a, Result<ScanResult>>;
}

/// Trait for performance checking.
///
/// Implementors analyze resource usage and efficiency.
pub trait PerformanceChecker: Send + Sync {
    /// Check performance characteristics of an artifact.
    fn check<'a>(&'a self, artifact: &'a Artifact) -> BoxFuture<'a, Result<PerfResult>>;
}

/// Trait for accessibility checking.
pub trait AccessibilityChecker: Send + Sync {
    /// Check accessibility compliance of an artifact.
    fn check<'a>(&'a self, artifact: &'a Artifact) -> BoxFuture<'a, Result<A11yResult>>;
}

/// Result of a lint check.
#[derive(Debug, Clone, PartialEq)]
pub enum LintResult {
    /// No issues found.
    Pass,
    /// Minor issues found; artifact is acceptable.
    Warning(Vec<String>),
    /// Critical issues found; artifact should be rejected.
    Error(Vec<String>),
}

/// Result of a security scan.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanResult {
    /// No vulnerabilities found.
    Clean,
    /// Vulnerabilities detected.
    Vulnerabilities(Vec<String>),
}

/// Result of a performance check.
#[derive(Debug, Clone, PartialEq)]
pub enum PerfResult {
    /// Performance is acceptable.
    Acceptable,
    /// Performance issues found.
    Issues(Vec<String>),
}

/// Result of an accessibility check.
#[derive(Debug, Clone, PartialEq)]
pub enum A11yResult {
    /// Meets accessibility standards.
    Pass,
    /// Accessibility issues found.
    Issues(Vec<String>),
}

/// Aggregated verdict produced by a [`QualityGate`] run.
#[derive(Debug, Clone, PartialEq)]
pub struct QualityResult {
    /// Whether the artifact passed (no errors).
    pub passed: bool,
    /// Non-fatal issues surfaced by checkers.
    pub warnings: Vec<String>,
    /// Fatal issues that caused the gate to fail.
    pub errors: Vec<String>,
    /// Lint score in `[0.0, 1.0]`.
    pub lint_score: f64,
    /// Security score in `[0.0, 1.0]`.
    pub security_score: f64,
    /// Performance score in `[0.0, 1.0]`.
    pub performance_score: f64,
    /// Accessibility score in `[0.0, 1.0]`.
    pub accessibility_score: f64,
}

/// Quality gate that runs multiple checkers against an artifact.
///
/// Aggregates results from linters, security scanners, performance checkers,
/// and accessibility checkers to produce an overall quality verdict.
#[derive(Default)]
pub struct QualityGate {
    pub linters: Vec<Box<dyn Linter>>,
    pub security_scanners: Vec<Box<dyn SecurityScanner>>,
    pub performance_checkers: Vec<Box<dyn PerformanceChecker>>,
    pub accessibility_checkers: Vec<Box<dyn AccessibilityChecker>>,
}

impl QualityGate {
    /// Create a new empty quality gate.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a linter to the gate.
    pub fn with_linter(mut self, linter: Box<dyn Linter>) -> Self {
        self.linters.push(linter);
        self
    }

    /// Add a security scanner to the gate.
    pub fn with_security_scanner(mut self, scanner: Box<dyn SecurityScanner>) -> Self {
        self.security_scanners.push(scanner);
        self
    }

    /// Add a performance checker to the gate.
    pub fn with_performance_checker(mut self, checker: Box<dyn PerformanceChecker>) -> Self {
        self.performance_checkers.push(checker);
        self
    }

    /// Add an accessibility checker to the gate.
    pub fn with_accessibility_checker(mut self, checker: Box<dyn AccessibilityChecker>) -> Self {
        self.accessibility_checkers.push(checker);
        self
    }

    /// Run all checkers against an artifact.
    ///
    /// Returns a `QualityResult` with detailed scores and any warnings/errors.
    /// If any security scanner finds vulnerabilities or any linter returns Error,
    /// the gate fails (`passed: false`).
    #[instrument(skip(self))]
    pub async fn check(&self, artifact: &Artifact) -> Result<QualityResult> {
        info!(artifact = %artifact_label(artifact), "Running quality gate");

        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        // Run linters
        let mut lint_score: f64 = 1.0;
        for linter in &self.linters {
            match linter.lint(artifact).await? {
                LintResult::Pass => {}
                LintResult::Warning(w) => {
                    warnings.extend(w.clone());
                    lint_score = lint_score.min(0.8);
                }
                LintResult::Error(e) => {
                    errors.extend(e.clone());
                    lint_score = 0.0;
                }
            }
        }

        // Run security scanners
        let mut security_score = 1.0;
        for scanner in &self.security_scanners {
            match scanner.scan(artifact).await? {
                ScanResult::Clean => {}
                ScanResult::Vulnerabilities(v) => {
                    errors.extend(v.clone());
                    security_score = 0.0;
                }
            }
        }

        // Run performance checkers
        let mut performance_score: f64 = 1.0;
        for checker in &self.performance_checkers {
            match checker.check(artifact).await? {
                PerfResult::Acceptable => {}
                PerfResult::Issues(i) => {
                    warnings.extend(i.clone());
                    performance_score = performance_score.min(0.7);
                }
            }
        }

        // Run accessibility checkers
        let mut accessibility_score: f64 = 1.0;
        for checker in &self.accessibility_checkers {
            match checker.check(artifact).await? {
                A11yResult::Pass => {}
                A11yResult::Issues(i) => {
                    warnings.extend(i.clone());
                    accessibility_score = accessibility_score.min(0.8);
                }
            }
        }

        let passed = errors.is_empty();

        info!(
            passed = %passed,
            warnings = %warnings.len(),
            errors = %errors.len(),
            "Quality gate complete"
        );

        Ok(QualityResult {
            passed,
            warnings,
            errors,
            lint_score,
            security_score,
            performance_score,
            accessibility_score,
        })
    }

    /// Quick check: run with no registered checkers (always passes).
    pub async fn check_minimal(&self, artifact: &Artifact) -> Result<QualityResult> {
        info!(artifact = %artifact_label(artifact), "Running minimal quality gate (no checkers)");
        Ok(QualityResult {
            passed: true,
            warnings: Vec::new(),
            errors: Vec::new(),
            lint_score: 1.0,
            security_score: 1.0,
            performance_score: 1.0,
            accessibility_score: 1.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_artifact(content: &str) -> Artifact {
        Artifact::Code {
            path: "test".to_string(),
            content: content.to_string(),
            language: "rust".to_string(),
        }
    }

    struct AlwaysPassLinter;
    impl Linter for AlwaysPassLinter {
        fn lint<'a>(&'a self, _artifact: &'a Artifact) -> BoxFuture<'a, Result<LintResult>> {
            Box::pin(async { Ok(LintResult::Pass) })
        }
    }

    struct AlwaysWarnLinter;
    impl Linter for AlwaysWarnLinter {
        fn lint<'a>(&'a self, _artifact: &'a Artifact) -> BoxFuture<'a, Result<LintResult>> {
            Box::pin(async { Ok(LintResult::Warning(vec!["Missing docs".to_string()])) })
        }
    }

    struct AlwaysErrorLinter;
    impl Linter for AlwaysErrorLinter {
        fn lint<'a>(&'a self, _artifact: &'a Artifact) -> BoxFuture<'a, Result<LintResult>> {
            Box::pin(async { Ok(LintResult::Error(vec!["Syntax error".to_string()])) })
        }
    }

    struct AlwaysCleanScanner;
    impl SecurityScanner for AlwaysCleanScanner {
        fn scan<'a>(&'a self, _artifact: &'a Artifact) -> BoxFuture<'a, Result<ScanResult>> {
            Box::pin(async { Ok(ScanResult::Clean) })
        }
    }

    struct AlwaysVulnScanner;
    impl SecurityScanner for AlwaysVulnScanner {
        fn scan<'a>(&'a self, _artifact: &'a Artifact) -> BoxFuture<'a, Result<ScanResult>> {
            Box::pin(async {
                Ok(ScanResult::Vulnerabilities(vec![
                    "SQL injection risk".to_string(),
                ]))
            })
        }
    }

    struct AlwaysAcceptablePerf;
    impl PerformanceChecker for AlwaysAcceptablePerf {
        fn check<'a>(&'a self, _artifact: &'a Artifact) -> BoxFuture<'a, Result<PerfResult>> {
            Box::pin(async { Ok(PerfResult::Acceptable) })
        }
    }

    struct AlwaysIssuePerf;
    impl PerformanceChecker for AlwaysIssuePerf {
        fn check<'a>(&'a self, _artifact: &'a Artifact) -> BoxFuture<'a, Result<PerfResult>> {
            Box::pin(async { Ok(PerfResult::Issues(vec!["Slow loop detected".to_string()])) })
        }
    }

    struct AlwaysPassA11y;
    impl AccessibilityChecker for AlwaysPassA11y {
        fn check<'a>(&'a self, _artifact: &'a Artifact) -> BoxFuture<'a, Result<A11yResult>> {
            Box::pin(async { Ok(A11yResult::Pass) })
        }
    }

    struct AlwaysIssueA11y;
    impl AccessibilityChecker for AlwaysIssueA11y {
        fn check<'a>(&'a self, _artifact: &'a Artifact) -> BoxFuture<'a, Result<A11yResult>> {
            Box::pin(async { Ok(A11yResult::Issues(vec!["Missing alt text".to_string()])) })
        }
    }

    #[tokio::test]
    async fn test_empty_gate_passes() {
        let gate = QualityGate::new();
        let artifact = make_artifact("fn main() {}");
        let result = gate.check(&artifact).await.unwrap();
        assert!(result.passed);
        assert!(result.warnings.is_empty());
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_linter_pass() {
        let gate = QualityGate::new().with_linter(Box::new(AlwaysPassLinter));
        let artifact = make_artifact("fn main() {}");
        let result = gate.check(&artifact).await.unwrap();
        assert!(result.passed);
        assert_eq!(result.lint_score, 1.0);
    }

    #[tokio::test]
    async fn test_linter_warning() {
        let gate = QualityGate::new().with_linter(Box::new(AlwaysWarnLinter));
        let artifact = make_artifact("fn main() {}");
        let result = gate.check(&artifact).await.unwrap();
        assert!(result.passed);
        assert!(!result.warnings.is_empty());
        assert_eq!(result.lint_score, 0.8);
    }

    #[tokio::test]
    async fn test_linter_error_fails_gate() {
        let gate = QualityGate::new().with_linter(Box::new(AlwaysErrorLinter));
        let artifact = make_artifact("fn main() {}");
        let result = gate.check(&artifact).await.unwrap();
        assert!(!result.passed);
        assert!(!result.errors.is_empty());
        assert_eq!(result.lint_score, 0.0);
    }

    #[tokio::test]
    async fn test_security_vuln_fails_gate() {
        let gate = QualityGate::new().with_security_scanner(Box::new(AlwaysVulnScanner));
        let artifact = make_artifact("query = 'SELECT * FROM users WHERE id = ' + user_input");
        let result = gate.check(&artifact).await.unwrap();
        assert!(!result.passed);
        assert_eq!(result.security_score, 0.0);
    }

    #[tokio::test]
    async fn test_security_clean_passes() {
        let gate = QualityGate::new().with_security_scanner(Box::new(AlwaysCleanScanner));
        let artifact = make_artifact("fn main() {}");
        let result = gate.check(&artifact).await.unwrap();
        assert!(result.passed);
        assert_eq!(result.security_score, 1.0);
    }

    #[tokio::test]
    async fn test_performance_warning() {
        let gate = QualityGate::new().with_performance_checker(Box::new(AlwaysIssuePerf));
        let artifact = make_artifact("fn main() {}");
        let result = gate.check(&artifact).await.unwrap();
        assert!(result.passed); // Warnings don't fail the gate
        assert!(!result.warnings.is_empty());
        assert_eq!(result.performance_score, 0.7);
    }

    #[tokio::test]
    async fn test_accessibility_issues() {
        let gate = QualityGate::new().with_accessibility_checker(Box::new(AlwaysIssueA11y));
        let artifact = make_artifact("<img src='photo.jpg'>");
        let result = gate.check(&artifact).await.unwrap();
        assert!(result.passed); // Warnings don't fail the gate
        assert!(!result.warnings.is_empty());
        assert_eq!(result.accessibility_score, 0.8);
    }

    #[tokio::test]
    async fn test_combined_checkers() {
        let gate = QualityGate::new()
            .with_linter(Box::new(AlwaysPassLinter))
            .with_security_scanner(Box::new(AlwaysCleanScanner))
            .with_performance_checker(Box::new(AlwaysAcceptablePerf))
            .with_accessibility_checker(Box::new(AlwaysPassA11y));

        let artifact = make_artifact("fn main() {}");
        let result = gate.check(&artifact).await.unwrap();
        assert!(result.passed);
        assert_eq!(result.lint_score, 1.0);
        assert_eq!(result.security_score, 1.0);
        assert_eq!(result.performance_score, 1.0);
        assert_eq!(result.accessibility_score, 1.0);
    }

    #[tokio::test]
    async fn test_security_error_overrides_passing_linter() {
        let gate = QualityGate::new()
            .with_linter(Box::new(AlwaysPassLinter))
            .with_security_scanner(Box::new(AlwaysVulnScanner));

        let artifact = make_artifact("fn main() {}");
        let result = gate.check(&artifact).await.unwrap();
        assert!(!result.passed);
        assert_eq!(result.lint_score, 1.0);
        assert_eq!(result.security_score, 0.0);
    }
}
