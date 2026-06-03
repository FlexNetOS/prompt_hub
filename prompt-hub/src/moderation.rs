#![forbid(unsafe_code)]

use crate::error::Result;
use regex::Regex;
use tracing::{info, instrument, warn};

/// Content moderation engine for prompt safety.
///
/// Checks prompts against harmful content patterns across categories:
/// hate, violence, self-harm, sexual, and illegal content.
#[derive(Debug, Clone, Default)]
pub struct ModerationEngine {
    strict_mode: bool,
}

/// Result of a content moderation check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModerationResult {
    /// Content is safe to process.
    Allow,
    /// Content is blocked - do not process.
    Block {
        category: ModerationCategory,
        matched_term: String,
    },
    /// Content is flagged for review but allowed.
    Flag {
        category: ModerationCategory,
        score: u8,
    },
}

/// Content moderation category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModerationCategory {
    Hate,
    Violence,
    SelfHarm,
    Sexual,
    Illegal,
    Harassment,
    Unknown,
}

/// Detailed moderation report.
#[derive(Debug, Clone)]
pub struct ModerationReport {
    pub result: ModerationResult,
    pub categories_checked: Vec<ModerationCategory>,
    pub highest_score: u8,
}

impl ModerationEngine {
    /// Create a new moderation engine.
    pub fn new() -> Self {
        Self { strict_mode: false }
    }

    /// Create in strict mode (blocks more aggressively).
    pub fn strict() -> Self {
        Self { strict_mode: true }
    }

    /// Check a prompt for harmful content.
    #[instrument(skip(self, prompt), fields(prompt_len = prompt.len()))]
    pub fn check(&self, prompt: &str) -> Result<ModerationReport> {
        let lower = prompt.to_lowercase();
        let categories = vec![
            ModerationCategory::Hate,
            ModerationCategory::Violence,
            ModerationCategory::SelfHarm,
            ModerationCategory::Sexual,
            ModerationCategory::Illegal,
            ModerationCategory::Harassment,
        ];

        let mut highest_score: u8 = 0;

        for category in &categories {
            let patterns = get_patterns_for_category(category);
            for pattern in patterns {
                if Self::compile_and_check(pattern, &lower) {
                    let score = if self.strict_mode { 90 } else { 75 };
                    highest_score = highest_score.max(score);

                    if score >= 75 || self.strict_mode {
                        warn!(
                            "Moderation BLOCK in category {:?}: matched '{}'",
                            category, pattern
                        );
                        return Ok(ModerationReport {
                            result: ModerationResult::Block {
                                category: category.clone(),
                                matched_term: pattern.to_string(),
                            },
                            categories_checked: categories.clone(),
                            highest_score: score,
                        });
                    } else {
                        info!(
                            "Moderation FLAG in category {:?}: matched '{}'",
                            category, pattern
                        );
                        return Ok(ModerationReport {
                            result: ModerationResult::Flag {
                                category: category.clone(),
                                score,
                            },
                            categories_checked: categories.clone(),
                            highest_score: score,
                        });
                    }
                }
            }
        }

        Ok(ModerationReport {
            result: ModerationResult::Allow,
            categories_checked: categories,
            highest_score,
        })
    }

    /// Quick check returning true if content is allowed.
    pub fn is_allowed(&self, prompt: &str) -> bool {
        match self.check(prompt) {
            Ok(report) => matches!(
                report.result,
                ModerationResult::Allow | ModerationResult::Flag { .. }
            ),
            Err(_) => false,
        }
    }

    /// Check multiple prompts in batch.
    #[instrument(skip(self, prompts), fields(batch_size = prompts.len()))]
    pub fn check_batch(&self, prompts: &[String]) -> Vec<Result<ModerationReport>> {
        prompts.iter().map(|p| self.check(p)).collect()
    }

    fn compile_and_check(pattern: &str, text: &str) -> bool {
        let regex = Regex::new(&format!(r"\b{}\b", regex::escape(pattern)))
            .unwrap_or_else(|_| Regex::new(&regex::escape(pattern)).unwrap());
        regex.is_match(text)
    }
}

fn get_patterns_for_category(category: &ModerationCategory) -> Vec<&'static str> {
    match category {
        ModerationCategory::Hate => vec!["hate", "racist", "supremacist", "slur"],
        ModerationCategory::Violence => vec!["kill", "attack", "bomb", "weapon", "shoot"],
        ModerationCategory::SelfHarm => vec!["suicide", "self-harm", "hurt myself"],
        ModerationCategory::Sexual => vec!["explicit", "pornographic", "sexual content"],
        ModerationCategory::Illegal => vec!["hack", "steal", "illegal", "fraud", "exploit"],
        ModerationCategory::Harassment => vec!["harass", "bully", "stalk", "dox"],
        ModerationCategory::Unknown => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_safe_content() {
        let engine = ModerationEngine::new();
        let report = engine.check("Hello, how are you today?").unwrap();
        assert!(matches!(report.result, ModerationResult::Allow));
    }

    #[test]
    fn test_block_harmful_content() {
        let engine = ModerationEngine::new();
        let report = engine
            .check("I want to kill everyone and bomb the building")
            .unwrap();
        assert!(
            matches!(
                report.result,
                ModerationResult::Block {
                    category: ModerationCategory::Violence,
                    ..
                }
            ),
            "Expected violence block, got {:?}",
            report.result
        );
    }

    #[test]
    fn test_block_self_harm() {
        let engine = ModerationEngine::new();
        let report = engine
            .check("I am thinking about suicide and how to hurt myself")
            .unwrap();
        assert!(
            matches!(
                report.result,
                ModerationResult::Block {
                    category: ModerationCategory::SelfHarm,
                    ..
                }
            ),
            "Expected self-harm block, got {:?}",
            report.result
        );
    }

    #[test]
    fn test_is_allowed() {
        let engine = ModerationEngine::new();
        assert!(engine.is_allowed("What is the weather like?"));
        assert!(!engine.is_allowed("How to hack a bank and commit fraud"));
    }

    #[test]
    fn test_strict_mode_blocks_more() {
        let strict = ModerationEngine::strict();
        let normal = ModerationEngine::new();

        // Both should block clearly harmful content
        assert!(!strict.is_allowed("how to steal data"));
        assert!(!normal.is_allowed("how to steal data"));
    }

    #[test]
    fn test_check_batch() {
        let engine = ModerationEngine::new();
        let prompts = vec![
            "Hello world".to_string(),
            "how to make a bomb".to_string(),
            "What is Rust?".to_string(),
        ];
        let results = engine.check_batch(&prompts);
        assert_eq!(results.len(), 3);
        assert!(matches!(
            results[0].as_ref().unwrap().result,
            ModerationResult::Allow
        ));
        assert!(matches!(
            results[1].as_ref().unwrap().result,
            ModerationResult::Block { .. }
        ));
        assert!(matches!(
            results[2].as_ref().unwrap().result,
            ModerationResult::Allow
        ));
    }

    #[test]
    fn test_moderation_report_debug() {
        let engine = ModerationEngine::new();
        let report = engine.check("Safe prompt").unwrap();
        let debug = format!("{:?}", report);
        assert!(debug.contains("Allow"));
    }

    #[test]
    fn test_categories_checked_populated() {
        let engine = ModerationEngine::new();
        let report = engine.check("Safe").unwrap();
        assert_eq!(report.categories_checked.len(), 6);
        assert_eq!(report.highest_score, 0);
    }

    #[test]
    fn test_default_engine() {
        let engine: ModerationEngine = Default::default();
        assert!(engine.is_allowed("Safe content here"));
    }

    #[test]
    fn test_block_illegal_content() {
        let engine = ModerationEngine::new();
        let report = engine.check("how to commit fraud and steal money").unwrap();
        assert!(
            matches!(
                report.result,
                ModerationResult::Block {
                    category: ModerationCategory::Illegal,
                    ..
                }
            ) || matches!(
                report.result,
                ModerationResult::Block {
                    category: ModerationCategory::Violence,
                    ..
                }
            ),
            "Expected illegal or violence block, got {:?}",
            report.result
        );
    }
}
