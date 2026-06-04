#![forbid(unsafe_code)]

use crate::error::Result;
use tracing::{info, instrument};

/// Token counting engine supporting multiple strategies.
///
/// When the `tiktoken` feature is enabled, uses `tiktoken-rs` for accurate
/// token counting. Otherwise falls back to a character-based approximation
/// (~4 chars per token for GPT-style tokenizers).
#[derive(Debug, Clone, Default)]
pub struct TokenCounter;

/// Result of counting tokens for a given model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenCount {
    /// Model identifier the count was computed for.
    pub model: String,
    /// Number of tokens counted.
    pub tokens: usize,
}

/// Cost estimate for a given token count and model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CostEstimateDetail {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub input_cost: f64,
    pub output_cost: f64,
    pub total_cost: f64,
    pub model: String,
}

impl TokenCounter {
    /// Count tokens in a text string.
    ///
    /// Uses tiktoken-rs when available, otherwise falls back to
    /// a whitespace-and-character heuristic.
    #[instrument]
    pub fn count_text(text: &str, model: &str) -> Result<TokenCount> {
        let tokens = if cfg!(feature = "tiktoken") {
            // When tiktoken-rs is available, use its encoder
            #[cfg(feature = "tiktoken")]
            {
                use tiktoken_rs::bpe_for_model;
                let bpe = bpe_for_model(model)
                    .map_err(|e| crate::error::HubError::Internal(format!("tiktoken: {e}")))?;
                // Keep the same "at least 1 token" contract as the heuristic
                // path below (tiktoken encodes an empty string to 0 tokens).
                bpe.encode_ordinary(text).len().max(1)
            }
            #[cfg(not(feature = "tiktoken"))]
            {
                unreachable!("tiktoken feature branch but feature not enabled")
            }
        } else {
            // Rough approximation: ~4 characters per token for English text
            // Whitespace-based heuristic: words * 4/3
            let word_count = text.split_whitespace().count();
            let char_estimate = text.len() / 4;
            let word_estimate = word_count * 4 / 3;
            word_estimate.max(char_estimate)
        };

        // Ensure we always return at least 1 token (conservative estimate)
        let tokens = tokens.max(1);

        info!(model = %model, tokens = %tokens, "Counted tokens");

        Ok(TokenCount {
            model: model.to_string(),
            tokens,
        })
    }

    /// Count tokens for a prompt (system + user combined).
    #[instrument]
    pub async fn count_prompt(system: &str, user: &str, model: &str) -> Result<TokenCount> {
        let combined = format!("{}\n{}", system, user);
        Self::count_text(&combined, model)
    }

    /// Estimate cost for a given token count.
    ///
    /// Pricing is approximate and based on common OpenAI pricing tiers.
    pub fn estimate_cost(
        input_tokens: usize,
        output_tokens: usize,
        model: &str,
    ) -> CostEstimateDetail {
        let (input_price_per_1k, output_price_per_1k) = Self::pricing(model);

        let input_cost = input_tokens as f64 * input_price_per_1k / 1000.0;
        let output_cost = output_tokens as f64 * output_price_per_1k / 1000.0;

        CostEstimateDetail {
            input_tokens,
            output_tokens,
            input_cost,
            output_cost,
            total_cost: input_cost + output_cost,
            model: model.to_string(),
        }
    }

    /// Estimate cost for a prompt before sending it to an LLM.
    #[instrument]
    pub async fn estimate_prompt_cost(
        system: &str,
        user: &str,
        model: &str,
        expected_output_tokens: usize,
    ) -> Result<CostEstimateDetail> {
        let input_count = Self::count_text(&format!("{}\n{}", system, user), model)?;
        Ok(Self::estimate_cost(
            input_count.tokens,
            expected_output_tokens,
            model,
        ))
    }

    /// Get approximate pricing per 1K tokens for known models.
    ///
    /// Returns (input_price, output_price) in USD.
    fn pricing(model: &str) -> (f64, f64) {
        let lower = model.to_lowercase();
        if lower.contains("gpt-4") && lower.contains("o1") {
            // o1 models
            (15.0, 60.0)
        } else if lower.contains("gpt-4-turbo") || lower.contains("gpt-4-0125") {
            (10.0, 30.0)
        } else if lower.contains("gpt-4") {
            // Standard GPT-4
            (30.0, 60.0)
        } else if lower.contains("gpt-3.5-turbo") || lower.contains("gpt-3.5") {
            (0.0005, 0.0015)
        } else if lower.contains("claude-3-opus") {
            (15.0, 75.0)
        } else if lower.contains("claude-3-sonnet") {
            (3.0, 15.0)
        } else if lower.contains("claude-3-haiku") {
            (0.25, 1.25)
        } else {
            // Default conservative estimate
            (5.0, 15.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_text_basic() {
        let result = TokenCounter::count_text("Hello world", "gpt-4").unwrap();
        assert_eq!(result.model, "gpt-4");
        assert!(result.tokens > 0, "Should count at least 1 token");
    }

    #[test]
    fn test_count_text_empty() {
        let result = TokenCounter::count_text("", "gpt-4").unwrap();
        // Empty string should still return at least 1 due to max(1)
        assert!(result.tokens >= 1);
    }

    #[test]
    fn test_count_text_longer_has_more_tokens() {
        let short = TokenCounter::count_text("Hi", "gpt-4").unwrap();
        let long = TokenCounter::count_text(
            "This is a much longer sentence with many words in it.",
            "gpt-4",
        )
        .unwrap();
        assert!(
            long.tokens > short.tokens,
            "Longer text should have more tokens: {} vs {}",
            long.tokens,
            short.tokens
        );
    }

    #[tokio::test]
    async fn test_count_prompt() {
        let result = TokenCounter::count_prompt("System prompt here", "User message here", "gpt-4")
            .await
            .unwrap();
        assert_eq!(result.model, "gpt-4");
        assert!(result.tokens > 0);
    }

    #[test]
    fn test_estimate_cost_gpt4() {
        let estimate = TokenCounter::estimate_cost(1000, 500, "gpt-4");
        assert_eq!(estimate.input_tokens, 1000);
        assert_eq!(estimate.output_tokens, 500);
        assert!(estimate.input_cost > 0.0);
        assert!(estimate.output_cost > 0.0);
        assert_eq!(
            estimate.total_cost,
            estimate.input_cost + estimate.output_cost
        );
        assert_eq!(estimate.model, "gpt-4");
    }

    #[test]
    fn test_estimate_cost_gpt35() {
        let estimate = TokenCounter::estimate_cost(1000, 500, "gpt-3.5-turbo");
        // GPT-3.5 is cheaper than GPT-4
        assert!(
            estimate.total_cost < 1.0,
            "GPT-3.5 should be cheap: {}",
            estimate.total_cost
        );
    }

    #[test]
    fn test_estimate_cost_claude() {
        let estimate_opus = TokenCounter::estimate_cost(1000, 500, "claude-3-opus");
        let estimate_haiku = TokenCounter::estimate_cost(1000, 500, "claude-3-haiku");
        // Opus should be more expensive than Haiku
        assert!(
            estimate_opus.total_cost > estimate_haiku.total_cost,
            "Opus should be more expensive than Haiku"
        );
    }

    #[test]
    fn test_estimate_cost_unknown_model() {
        // Should use default pricing
        let estimate = TokenCounter::estimate_cost(1000, 500, "some-unknown-model-v1");
        assert!(
            estimate.total_cost > 0.0,
            "Unknown model should use default pricing"
        );
    }

    #[tokio::test]
    async fn test_estimate_prompt_cost() {
        let estimate = TokenCounter::estimate_prompt_cost(
            "You are a helpful assistant.",
            "Write a poem about Rust.",
            "gpt-3.5-turbo",
            200,
        )
        .await
        .unwrap();
        assert!(estimate.input_tokens > 0);
        assert_eq!(estimate.output_tokens, 200);
        assert!(estimate.total_cost > 0.0);
    }
}
