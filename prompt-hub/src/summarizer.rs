#![forbid(unsafe_code)]

use crate::error::Result;
use crate::models::{ExecutionResult, SkillLevel};
use tracing::{info, instrument};

/// Summarizes execution results in plain English tailored to the user's skill level.
///
/// Beginners get friendly, verbose explanations with suggestions.
/// Experts get concise, technical output with exact file paths and costs.
#[derive(Debug, Clone, Default)]
pub struct ResultSummarizer;

impl ResultSummarizer {
    /// Summarize an execution result at the appropriate level for the user.
    #[instrument]
    pub async fn summarize(
        &self,
        result: &ExecutionResult,
        user_level: SkillLevel,
    ) -> Result<String> {
        info!("Summarizing result for {:?} user", user_level);

        let summary = match user_level {
            SkillLevel::Beginner => self.beginner_summary(result),
            SkillLevel::Intermediate => self.intermediate_summary(result),
            SkillLevel::Expert => self.expert_summary(result),
        };

        Ok(summary)
    }

    /// Generate a beginner-friendly summary with lots of context and encouragement.
    fn beginner_summary(&self, result: &ExecutionResult) -> String {
        let time_taken = result.duration.as_secs();
        let suggestions = if result.next_suggestions.is_empty() {
            "No suggestions available.".to_string()
        } else {
            result
                .next_suggestions
                .iter()
                .map(|s| format!("  • {}", s))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let what_happened = if result.actions.is_empty() {
            result.reasoning.clone()
        } else {
            result
                .actions
                .iter()
                .map(|a| format!("  ✅ {}", a))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let files_section = if result.files_changed.is_empty() {
            ""
        } else {
            &format!(
                "\n\n📁 Files created or modified:\n{}",
                result
                    .files_changed
                    .iter()
                    .map(|f| format!("  • {}", f))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };

        format!(
            "🎉 All done! Here's what I built for you:\n\n{}\n{}\n\n\
             ⏱️  Time taken: {} second{}\n\n\
             🤔 Want to add something next?\n{}",
            what_happened,
            files_section,
            time_taken,
            if time_taken == 1 { "" } else { "s" },
            suggestions
        )
    }

    /// Generate an intermediate summary with technical details and cost breakdown.
    fn intermediate_summary(&self, result: &ExecutionResult) -> String {
        let time_taken = result.duration.as_secs();

        let actions = if result.actions.is_empty() {
            "  (no actions recorded)".to_string()
        } else {
            result
                .actions
                .iter()
                .map(|a| format!("  • {}", a))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let files = if result.files_changed.is_empty() {
            "  (no files changed)".to_string()
        } else {
            result
                .files_changed
                .iter()
                .map(|f| format!("  • {}", f))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let suggestions = if result.next_suggestions.is_empty() {
            String::new()
        } else {
            format!(
                "\n\nNext steps:\n{}",
                result
                    .next_suggestions
                    .iter()
                    .map(|s| format!("  • {}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };

        format!(
            "Completed successfully in {}s.\n\n\
             Actions:\n{}\n\n\
             Files changed:\n{}\n\n\
             Cost: ${:.4}{}",
            time_taken, actions, files, result.token_cost, suggestions
        )
    }

    /// Generate an expert-level summary: minimal, precise, technical.
    fn expert_summary(&self, result: &ExecutionResult) -> String {
        let time_taken = result.duration.as_secs();
        let actions = result.actions.join(", ");
        let files = result.files_changed.join(", ");

        let actions_part = if actions.is_empty() {
            "(no actions)".to_string()
        } else {
            actions
        };

        let files_part = if files.is_empty() {
            "(no files changed)".to_string()
        } else {
            files
        };

        format!(
            "Done. {}s, ${:.4}\n\n{}\n{}",
            time_taken, result.token_cost, actions_part, files_part
        )
    }

    /// Quick summary without skill-level adaptation (uses intermediate level).
    pub fn summarize_quick(&self, result: &ExecutionResult) -> String {
        self.intermediate_summary(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_result() -> ExecutionResult {
        ExecutionResult {
            success: true,
            actions: vec![
                "Created login page".to_string(),
                "Added JWT auth middleware".to_string(),
                "Set up database migration".to_string(),
            ],
            reasoning: "Built a complete login system".to_string(),
            files_changed: vec![
                "src/auth.rs".to_string(),
                "src/routes/login.rs".to_string(),
                "migrations/001_auth.sql".to_string(),
            ],
            next_suggestions: vec![
                "Add password reset".to_string(),
                "Enable email verification".to_string(),
                "Add social login".to_string(),
            ],
            duration: Duration::from_secs(18),
            token_cost: 0.0012,
        }
    }

    fn minimal_result() -> ExecutionResult {
        ExecutionResult {
            success: true,
            actions: vec![],
            reasoning: "Simple task completed".to_string(),
            files_changed: vec![],
            next_suggestions: vec![],
            duration: Duration::ZERO,
            token_cost: 0.0,
        }
    }

    #[tokio::test]
    async fn test_beginner_summary() {
        let summarizer = ResultSummarizer::default();
        let summary = summarizer
            .summarize(&test_result(), SkillLevel::Beginner)
            .await
            .unwrap();

        assert!(summary.contains("🎉 All done!"));
        assert!(summary.contains("login page"));
        assert!(summary.contains("JWT auth"));
        assert!(summary.contains("src/auth.rs"));
        assert!(summary.contains("Want to add something next?"));
        assert!(summary.contains("password reset"));
    }

    #[tokio::test]
    async fn test_intermediate_summary() {
        let summarizer = ResultSummarizer::default();
        let summary = summarizer
            .summarize(&test_result(), SkillLevel::Intermediate)
            .await
            .unwrap();

        assert!(summary.contains("Completed successfully"));
        assert!(summary.contains("18s"));
        assert!(summary.contains("$0.0012"));
        assert!(summary.contains("src/auth.rs"));
        assert!(summary.contains("Next steps"));
    }

    #[tokio::test]
    async fn test_expert_summary() {
        let summarizer = ResultSummarizer::default();
        let summary = summarizer
            .summarize(&test_result(), SkillLevel::Expert)
            .await
            .unwrap();

        assert!(summary.starts_with("Done."));
        assert!(summary.contains("18s"));
        assert!(summary.contains("$0.0012"));
        assert!(summary.contains("src/auth.rs"));
        // Expert output should be concise — no emoji
        assert!(!summary.contains("🎉"));
    }

    #[tokio::test]
    async fn test_beginner_summary_minimal_result() {
        let summarizer = ResultSummarizer::default();
        let summary = summarizer
            .summarize(&minimal_result(), SkillLevel::Beginner)
            .await
            .unwrap();

        assert!(summary.contains("🎉 All done!"));
        // Should not panic on empty actions/files
        assert!(summary.contains("Simple task completed"));
    }

    #[tokio::test]
    async fn test_expert_summary_minimal_result() {
        let summarizer = ResultSummarizer::default();
        let summary = summarizer
            .summarize(&minimal_result(), SkillLevel::Expert)
            .await
            .unwrap();

        assert!(summary.contains("0s"));
        assert!(summary.contains("$0.0000"));
    }

    #[test]
    fn test_summarize_quick() {
        let summarizer = ResultSummarizer::default();
        let summary = summarizer.summarize_quick(&test_result());

        assert!(summary.contains("Completed successfully"));
        assert!(summary.contains("18s"));
    }

    #[test]
    fn test_beginner_grammar_singular() {
        let mut result = test_result();
        result.duration = Duration::from_secs(1);

        let summarizer = ResultSummarizer::default();
        let summary = summarizer.beginner_summary(&result);

        // Should say "1 second" not "1 seconds"
        assert!(summary.contains("1 second"));
        assert!(!summary.contains("1 seconds"));
    }
}
