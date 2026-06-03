#![forbid(unsafe_code)]

use crate::error::Result;
use crate::models::{Complexity, CostEstimate, Intent, ProjectContext};
use tracing::instrument;

/// Cost estimation engine.
///
/// Estimates token usage, time, and USD cost before executing a request,
/// enabling the user to make informed decisions.
#[derive(Debug, Clone, Default)]
pub struct CostEstimator;

impl CostEstimator {
    /// Estimate cost for an intent given project context.
    ///
    /// Estimates are based on complexity tier and adjusted for project size.
    #[instrument]
    pub async fn estimate(
        &self,
        intent: &Intent,
        context: &ProjectContext,
    ) -> Result<CostEstimate> {
        // Base estimates from complexity tier
        let (tokens_in, tokens_out, time_secs, cost) = match intent.complexity {
            Complexity::Simple => (5_000, 2_000, 60, 0.03),
            Complexity::Moderate => (15_000, 5_000, 180, 0.08),
            Complexity::Complex => (50_000, 15_000, 600, 0.25),
            Complexity::Research => (100_000, 30_000, 1_200, 0.50),
        };

        // Adjust for project size (more files = more context to process)
        let file_count = context.existing_files.len() as u64;
        let file_multiplier = if file_count > 100 {
            1.5
        } else if file_count > 50 {
            1.25
        } else if file_count > 20 {
            1.1
        } else {
            1.0
        };

        // Adjust for framework complexity
        let fw_multiplier = match context.framework.as_str() {
            "nextjs" | "angular" => 1.15,
            "rust" | "axum" | "actix-web" => 1.1,
            _ => 1.0,
        };

        let adjusted_tokens_in = (tokens_in as f64 * file_multiplier * fw_multiplier) as u64;
        let adjusted_cost = cost * file_multiplier * fw_multiplier;
        let adjusted_time = (time_secs as f64 * file_multiplier) as u32;

        // Adjust confidence based on context availability
        let confidence = if context.language == "unknown" {
            0.60 // Less confident without project context
        } else {
            0.75 + (0.05 * file_multiplier.min(1.4) - 0.05) // 0.75-0.82
        };

        Ok(CostEstimate {
            tokens_input: adjusted_tokens_in,
            tokens_output: tokens_out,
            cost_usd: adjusted_cost,
            estimated_cost_usd: adjusted_cost,
            time_seconds: adjusted_time,
            confidence: confidence.min(0.95),
        })
    }

    /// Quick estimate without project context (lower confidence).
    pub async fn estimate_quick(&self, intent: &Intent) -> Result<CostEstimate> {
        self.estimate(intent, &ProjectContext::default()).await
    }

    /// Format a human-readable cost summary.
    pub fn format_estimate(estimate: &CostEstimate) -> String {
        let minutes = estimate.time_seconds / 60;
        let seconds = estimate.time_seconds % 60;

        let time_str = if minutes > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}s", seconds)
        };

        format!(
            "Estimated: {} tokens in / {} tokens out | ~{} | ${:.2} | confidence: {:.0}%",
            estimate.tokens_input,
            estimate.tokens_output,
            time_str,
            estimate.cost_usd,
            estimate.confidence * 100.0
        )
    }

    /// Check if the estimated cost is within a given budget.
    pub fn within_budget(estimate: &CostEstimate, max_cost_usd: f64) -> bool {
        estimate.cost_usd <= max_cost_usd
    }

    /// Check if the estimated time is within a given limit.
    pub fn within_time_limit(estimate: &CostEstimate, max_seconds: u32) -> bool {
        estimate.time_seconds <= max_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Complexity, FileEntry};
    use chrono::Utc;

    fn test_intent(complexity: Complexity) -> Intent {
        Intent {
            complexity,
            ..Default::default()
        }
    }

    fn empty_context() -> ProjectContext {
        ProjectContext::default()
    }

    fn large_context(file_count: usize) -> ProjectContext {
        let mut ctx = ProjectContext {
            language: "typescript".to_string(),
            framework: "nextjs".to_string(),
            ..Default::default()
        };
        for i in 0..file_count {
            ctx.existing_files.push(FileEntry {
                path: format!("src/file{i}.ts"),
                size: 1000,
                modified: Utc::now(),
            });
        }
        ctx
    }

    #[tokio::test]
    async fn test_estimate_simple() {
        let estimator = CostEstimator::default();
        let intent = test_intent(Complexity::Simple);
        let ctx = empty_context();

        let estimate = estimator.estimate(&intent, &ctx).await.unwrap();

        assert_eq!(estimate.tokens_input, 5_000);
        assert_eq!(estimate.tokens_output, 2_000);
        assert_eq!(estimate.time_seconds, 60);
        assert!((estimate.cost_usd - 0.03).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_estimate_moderate() {
        let estimator = CostEstimator::default();
        let intent = test_intent(Complexity::Moderate);
        let ctx = empty_context();

        let estimate = estimator.estimate(&intent, &ctx).await.unwrap();

        assert_eq!(estimate.tokens_input, 15_000);
        assert_eq!(estimate.tokens_output, 5_000);
        assert_eq!(estimate.time_seconds, 180);
    }

    #[tokio::test]
    async fn test_estimate_complex() {
        let estimator = CostEstimator::default();
        let intent = test_intent(Complexity::Complex);
        let ctx = empty_context();

        let estimate = estimator.estimate(&intent, &ctx).await.unwrap();

        assert_eq!(estimate.tokens_input, 50_000);
        assert_eq!(estimate.tokens_output, 15_000);
        assert_eq!(estimate.time_seconds, 600);
    }

    #[tokio::test]
    async fn test_estimate_research() {
        let estimator = CostEstimator::default();
        let intent = test_intent(Complexity::Research);
        let ctx = empty_context();

        let estimate = estimator.estimate(&intent, &ctx).await.unwrap();

        assert_eq!(estimate.tokens_input, 100_000);
        assert_eq!(estimate.tokens_output, 30_000);
        assert_eq!(estimate.time_seconds, 1_200);
    }

    #[tokio::test]
    async fn test_estimate_with_large_project() {
        let estimator = CostEstimator::default();
        let intent = test_intent(Complexity::Moderate);
        let ctx = large_context(75); // 75 files = 1.25x multiplier

        let estimate = estimator.estimate(&intent, &ctx).await.unwrap();

        // 15_000 * 1.25 * 1.15 (nextjs) = 21,562
        assert!(estimate.tokens_input > 15_000);
        assert!(estimate.cost_usd > 0.08);
    }

    #[tokio::test]
    async fn test_estimate_quick() {
        let estimator = CostEstimator::default();
        let intent = test_intent(Complexity::Simple);

        let estimate = estimator.estimate_quick(&intent).await.unwrap();

        // With unknown context, confidence should be lower
        assert_eq!(estimate.tokens_input, 5_000);
        assert!(estimate.confidence < 0.70);
    }

    #[tokio::test]
    async fn test_estimate_confidence_with_context() {
        let estimator = CostEstimator::default();
        let intent = test_intent(Complexity::Moderate);
        let mut ctx = empty_context();
        ctx.language = "typescript".to_string();
        ctx.framework = "react".to_string();

        let estimate = estimator.estimate(&intent, &ctx).await.unwrap();

        // With known context, confidence should be >= 0.75
        assert!(estimate.confidence >= 0.75);
    }

    #[test]
    fn test_format_estimate() {
        let estimate = CostEstimate {
            tokens_input: 15_000,
            tokens_output: 5_000,
            cost_usd: 0.08,
            estimated_cost_usd: 0.08,
            time_seconds: 180,
            confidence: 0.82,
        };

        let formatted = CostEstimator::format_estimate(&estimate);
        assert!(formatted.contains("15_000 tokens in") || formatted.contains("15000 tokens in"));
        assert!(formatted.contains("3m 0s") || formatted.contains("180s"));
        assert!(formatted.contains("$0.08"));
        assert!(formatted.contains("82%") || formatted.contains("82"));
    }

    #[test]
    fn test_within_budget() {
        let estimate = CostEstimate {
            cost_usd: 0.08,
            ..Default::default()
        };

        assert!(CostEstimator::within_budget(&estimate, 0.10));
        assert!(CostEstimator::within_budget(&estimate, 0.08));
        assert!(!CostEstimator::within_budget(&estimate, 0.05));
    }

    #[test]
    fn test_within_time_limit() {
        let estimate = CostEstimate {
            time_seconds: 180,
            ..Default::default()
        };

        assert!(CostEstimator::within_time_limit(&estimate, 200));
        assert!(CostEstimator::within_time_limit(&estimate, 180));
        assert!(!CostEstimator::within_time_limit(&estimate, 100));
    }

    #[test]
    fn test_format_estimate_short_duration() {
        let estimate = CostEstimate {
            tokens_input: 5_000,
            tokens_output: 2_000,
            cost_usd: 0.03,
            estimated_cost_usd: 0.03,
            time_seconds: 45,
            confidence: 0.75,
        };

        let formatted = CostEstimator::format_estimate(&estimate);
        // Should show just seconds when under 60
        assert!(formatted.contains("45s"));
    }
}
