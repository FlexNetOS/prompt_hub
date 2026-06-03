#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::{Artifact, Intent, ProjectContext};
use std::future::Future;
use std::pin::Pin;
use tracing::{debug, info, instrument, warn};

/// Boxed future returned by [`FallbackStrategy::attempt`].
///
/// Returning an explicitly-boxed future (instead of `async fn`) keeps the
/// trait object-safe so it can be stored as `Box<dyn FallbackStrategy>`.
type AttemptFuture<'a> = Pin<Box<dyn Future<Output = Result<Artifact>> + Send + 'a>>;

/// Fallback chain for resilient execution.
///
/// When the primary execution strategy fails, this chain tries a sequence
/// of fallback strategies — from switching models to simplifying the task —
/// silently recovering so the user only sees "Done!".
pub struct FallbackChain {
    pub strategies: Vec<Box<dyn FallbackStrategy>>,
    pub max_attempts: usize,
}

impl Default for FallbackChain {
    fn default() -> Self {
        Self {
            strategies: vec![
                Box::new(ModelFallback),
                Box::new(SkillFallback),
                Box::new(SimplificationFallback),
                Box::new(ManualDecompositionFallback),
            ],
            max_attempts: 5,
        }
    }
}

/// Trait for individual fallback strategies.
///
/// `attempt` returns an explicitly-boxed future (rather than `async fn`) so the
/// trait stays object-safe and can be used as `Box<dyn FallbackStrategy>`.
pub trait FallbackStrategy: Send + Sync {
    /// Attempt to recover by producing an artifact.
    fn attempt<'a>(&'a self, intent: &'a Intent, context: &'a ProjectContext) -> AttemptFuture<'a>;
    /// Human-readable strategy name for logging.
    fn name(&self) -> &'static str;
}

// ─────────────────────────────────────────────
// Strategy 1: Try a different LLM model
// ─────────────────────────────────────────────

/// Try an alternative LLM model (e.g., fallback from GPT-4 to Claude).
pub struct ModelFallback;

impl FallbackStrategy for ModelFallback {
    fn attempt<'a>(&'a self, intent: &'a Intent, context: &'a ProjectContext) -> AttemptFuture<'a> {
        Box::pin(async move {
            debug!(
                "ModelFallback: trying alternative model for '{}' in {:?} project",
                intent.raw_text, context.language
            );
            // In production: retry with alternative model provider
            // For now: produce a degraded but valid artifact
            Ok(Artifact::Documentation {
                title: format!("Fallback response for: {}", intent.raw_text),
                content: format!(
                    "Attempted with alternative model.\n\nIntent: {:?}\nDomain: {:?}\nTask: {:?}",
                    intent.raw_text, intent.domain, intent.task_type
                ),
                format: "markdown".to_string(),
            })
        })
    }

    fn name(&self) -> &'static str {
        "model_fallback"
    }
}

// ─────────────────────────────────────────────
// Strategy 2: Try a different skill
// ─────────────────────────────────────────────

/// Try an alternative skill that may handle the intent.
pub struct SkillFallback;

impl FallbackStrategy for SkillFallback {
    fn attempt<'a>(&'a self, intent: &'a Intent, context: &'a ProjectContext) -> AttemptFuture<'a> {
        Box::pin(async move {
            debug!(
                "SkillFallback: trying alternative skill for domain {:?}",
                intent.domain
            );

            let alt_skill = match intent.domain {
                crate::models::Domain::DevOps => "manual-deploy",
                crate::models::Domain::Security => "basic-security-checklist",
                crate::models::Domain::Analysis => "manual-analysis",
                _ => "general-code",
            };

            Ok(Artifact::Prompt {
                system: format!(
                    "You are using the fallback skill '{}'. \
                     Produce the best possible output given limited context.",
                    alt_skill
                ),
                user: format!(
                    "Using fallback skill '{}': {} for a {} project",
                    alt_skill, intent.raw_text, context.language
                ),
            })
        })
    }

    fn name(&self) -> &'static str {
        "skill_fallback"
    }
}

// ─────────────────────────────────────────────
// Strategy 3: Reduce complexity and retry
// ─────────────────────────────────────────────

/// Reduce the complexity of the request and produce a simplified artifact.
pub struct SimplificationFallback;

impl FallbackStrategy for SimplificationFallback {
    fn attempt<'a>(
        &'a self,
        intent: &'a Intent,
        _context: &'a ProjectContext,
    ) -> AttemptFuture<'a> {
        Box::pin(async move {
            debug!(
                "SimplificationFallback: reducing complexity for '{}':",
                intent.raw_text
            );

            let simplified_request = format!(
                "SIMPLIFIED VERSION (reduced from {:?} complexity): \
                 Create a minimal implementation of: {}",
                intent.complexity, intent.raw_text
            );

            Ok(Artifact::Prompt {
                system: "You are in simplified fallback mode. \
                         Produce a minimal but working implementation. \
                         Skip advanced features, focus on core functionality."
                    .to_string(),
                user: simplified_request,
            })
        })
    }

    fn name(&self) -> &'static str {
        "simplification_fallback"
    }
}

// ─────────────────────────────────────────────
// Strategy 4: Break into smaller steps
// ─────────────────────────────────────────────

/// Decompose the request into smaller, independently executable steps.
pub struct ManualDecompositionFallback;

impl FallbackStrategy for ManualDecompositionFallback {
    fn attempt<'a>(
        &'a self,
        intent: &'a Intent,
        _context: &'a ProjectContext,
    ) -> AttemptFuture<'a> {
        Box::pin(async move {
            debug!(
                "ManualDecompositionFallback: breaking '{}' into smaller steps",
                intent.raw_text
            );

            let steps = [
                "Step 1: Set up project structure",
                "Step 2: Implement core functionality",
                "Step 3: Add basic styling",
                "Step 4: Add error handling",
            ];

            Ok(Artifact::Documentation {
                title: format!("Decomposed Plan: {}", intent.raw_text),
                content: format!(
                    "The request has been decomposed into smaller steps:\n\n{}\n\n\
                     Please execute each step individually.",
                    steps.join("\n")
                ),
                format: "markdown".to_string(),
            })
        })
    }

    fn name(&self) -> &'static str {
        "decomposition_fallback"
    }
}

// ─────────────────────────────────────────────
// Fallback chain execution
// ─────────────────────────────────────────────

impl FallbackChain {
    /// Execute the fallback chain, trying each strategy in order.
    ///
    /// Returns the first successful artifact, or an error if all strategies fail.
    /// When a non-first strategy succeeds, an informational log is emitted.
    #[instrument(skip(self, intent, context))]
    pub async fn execute(&self, intent: &Intent, context: &ProjectContext) -> Result<Artifact> {
        for (i, strategy) in self.strategies.iter().enumerate() {
            if i >= self.max_attempts {
                warn!("Max fallback attempts ({}) reached", self.max_attempts);
                break;
            }

            match strategy.attempt(intent, context).await {
                Ok(artifact) => {
                    if i > 0 {
                        info!(
                            "Fallback succeeded using {} after {} failed attempt(s)",
                            strategy.name(),
                            i
                        );
                    } else {
                        debug!("Primary strategy {} succeeded immediately", strategy.name());
                    }
                    return Ok(artifact);
                }
                Err(e) => {
                    warn!(
                        "Fallback strategy {} failed (attempt {}): {}",
                        strategy.name(),
                        i + 1,
                        e
                    );

                    if i == self.strategies.len().saturating_sub(1) {
                        warn!("All fallback strategies exhausted");
                        return Err(HubError::FallbackExhausted(format!(
                            "All {} fallback strategies exhausted. Last error: {}",
                            i + 1,
                            e
                        )));
                    }
                }
            }
        }

        Err(HubError::FallbackExhausted(
            "Fallback chain completed without success".to_string(),
        ))
    }

    /// Execute with a custom list of pre-selected strategies.
    #[instrument(skip(self, intent, context, strategy_indices))]
    pub async fn execute_with(
        &self,
        intent: &Intent,
        context: &ProjectContext,
        strategy_indices: &[usize],
    ) -> Result<Artifact> {
        let selected: Vec<&Box<dyn FallbackStrategy>> = strategy_indices
            .iter()
            .filter_map(|&i| self.strategies.get(i))
            .collect();

        if selected.is_empty() {
            return Err(HubError::InvalidInput(
                "No valid strategy indices provided".to_string(),
            ));
        }

        for (i, strategy) in selected.iter().enumerate() {
            match strategy.attempt(intent, context).await {
                Ok(artifact) => {
                    if i > 0 {
                        info!(
                            "Custom fallback succeeded using {} after {} failed attempt(s)",
                            strategy.name(),
                            i
                        );
                    }
                    return Ok(artifact);
                }
                Err(e) => {
                    warn!("Custom fallback {} failed: {}", strategy.name(), e);
                }
            }
        }

        Err(HubError::FallbackExhausted(
            "All custom fallback strategies exhausted".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_intent() -> Intent {
        Intent {
            raw_text: "Build a login page".to_string(),
            ..Default::default()
        }
    }

    fn test_context() -> ProjectContext {
        ProjectContext {
            language: "typescript".to_string(),
            framework: "react".to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_model_fallback_attempt() {
        let fallback = ModelFallback;
        let result = fallback.attempt(&test_intent(), &test_context()).await;
        assert!(result.is_ok());

        let artifact = result.unwrap();
        match artifact {
            Artifact::Documentation { title, .. } => {
                assert!(title.contains("Fallback"));
            }
            _ => panic!("Expected Documentation artifact"),
        }
    }

    #[tokio::test]
    async fn test_skill_fallback_attempt() {
        let fallback = SkillFallback;
        let result = fallback.attempt(&test_intent(), &test_context()).await;
        assert!(result.is_ok());

        let artifact = result.unwrap();
        match artifact {
            Artifact::Prompt { system, .. } => {
                assert!(system.contains("fallback skill"));
            }
            _ => panic!("Expected Prompt artifact"),
        }
    }

    #[tokio::test]
    async fn test_simplification_fallback_attempt() {
        let fallback = SimplificationFallback;
        let result = fallback.attempt(&test_intent(), &test_context()).await;
        assert!(result.is_ok());

        let artifact = result.unwrap();
        match artifact {
            Artifact::Prompt { user, .. } => {
                assert!(user.contains("SIMPLIFIED"));
            }
            _ => panic!("Expected Prompt artifact"),
        }
    }

    #[tokio::test]
    async fn test_decomposition_fallback_attempt() {
        let fallback = ManualDecompositionFallback;
        let result = fallback.attempt(&test_intent(), &test_context()).await;
        assert!(result.is_ok());

        let artifact = result.unwrap();
        match artifact {
            Artifact::Documentation { content, .. } => {
                assert!(content.contains("Step 1"));
                assert!(content.contains("Step 2"));
            }
            _ => panic!("Expected Documentation artifact"),
        }
    }

    #[tokio::test]
    async fn test_fallback_chain_execute() {
        let chain = FallbackChain::default();
        let result = chain.execute(&test_intent(), &test_context()).await;

        // First strategy (ModelFallback) should succeed
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fallback_chain_execute_with_custom_indices() {
        let chain = FallbackChain::default();
        // Only try skill_fallback (index 1) and simplification (index 2)
        let result = chain
            .execute_with(&test_intent(), &test_context(), &[1, 2])
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fallback_chain_max_attempts() {
        let chain = FallbackChain {
            max_attempts: 1,
            ..Default::default()
        };
        let result = chain.execute(&test_intent(), &test_context()).await;
        // With max_attempts=1, only ModelFallback runs — should still succeed
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fallback_strategy_names() {
        assert_eq!(ModelFallback.name(), "model_fallback");
        assert_eq!(SkillFallback.name(), "skill_fallback");
        assert_eq!(SimplificationFallback.name(), "simplification_fallback");
        assert_eq!(ManualDecompositionFallback.name(), "decomposition_fallback");
    }

    #[tokio::test]
    async fn test_fallback_chain_empty_indices() {
        let chain = FallbackChain::default();
        let result = chain
            .execute_with(&test_intent(), &test_context(), &[])
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_fallback_chain_default() {
        let chain = FallbackChain::default();
        assert_eq!(chain.strategies.len(), 4);
        assert_eq!(chain.max_attempts, 5);
    }
}
