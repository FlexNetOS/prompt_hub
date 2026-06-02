#![forbid(unsafe_code)]

use crate::error::Result;
use crate::models::*;
use std::collections::HashMap;
use tracing::{info, instrument};
use uuid::Uuid;

/// A reusable prompt pattern extracted from a prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub id: Uuid,
    pub structure: String,
    pub domains: Vec<Domain>,
    pub score: f64,
    pub usage_count: u64,
    pub agent_id: Uuid,
    pub example_snippet: String,
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            structure: String::new(),
            domains: Vec::new(),
            score: 0.0,
            usage_count: 0,
            agent_id: Uuid::new_v4(),
            example_snippet: String::new(),
        }
    }
}

/// Cross-agent prompt pattern sharing engine.
///
/// Agents extract successful patterns from prompts and share them
/// through a common pattern pool. Patterns are scored by frequency,
/// success rate, and cross-domain generality.
#[derive(Debug, Clone, Default)]
pub struct CrossAgentPollination {
    pattern_pool: HashMap<String, Pattern>,
}

impl CrossAgentPollination {
    /// Create a new pollination engine.
    pub fn new() -> Self {
        Self {
            pattern_pool: HashMap::new(),
        }
    }

    /// Extract reusable patterns from a prompt.
    ///
    /// Identifies structural patterns like "step-by-step", "chain-of-thought",
    /// and "few-shot" from the prompt content.
    #[instrument]
    pub fn extract_patterns(prompt: &Prompt) -> Vec<Pattern> {
        let mut patterns = Vec::new();

        // Extract structure patterns from the prompt
        if prompt.system_prompt.contains("step") || prompt.system_prompt.contains("Step") {
            patterns.push(Pattern {
                id: Uuid::new_v4(),
                structure: "step-by-step".to_string(),
                domains: vec![prompt.domain],
                score: prompt.metrics.success_rate,
                usage_count: prompt.metrics.usage_count,
                agent_id: prompt.author.id,
                example_snippet: prompt.system_prompt.clone(),
            });
        }

        if prompt.system_prompt.contains("example") || prompt.system_prompt.contains("Example") {
            patterns.push(Pattern {
                id: Uuid::new_v4(),
                structure: "few-shot".to_string(),
                domains: vec![prompt.domain],
                score: prompt.metrics.success_rate,
                usage_count: prompt.metrics.usage_count,
                agent_id: prompt.author.id,
                example_snippet: prompt.system_prompt.clone(),
            });
        }

        if prompt.system_prompt.contains("think")
            || prompt.system_prompt.contains("reason")
            || prompt.system_prompt.contains("Reason")
        {
            patterns.push(Pattern {
                id: Uuid::new_v4(),
                structure: "chain-of-thought".to_string(),
                domains: vec![prompt.domain],
                score: prompt.metrics.success_rate,
                usage_count: prompt.metrics.usage_count,
                agent_id: prompt.author.id,
                example_snippet: prompt.system_prompt.clone(),
            });
        }

        info!(
            prompt_id = %prompt.id,
            pattern_count = %patterns.len(),
            "Extracted patterns from prompt"
        );

        patterns
    }

    /// Share a pattern into the global pool.
    #[instrument]
    pub fn share_pattern(&mut self, pattern: Pattern) -> Result<()> {
        let key = pattern.id.to_string();
        info!(
            pattern_id = %key,
            structure = %pattern.structure,
            "Sharing pattern to pool"
        );
        self.pattern_pool.insert(key, pattern);
        Ok(())
    }

    /// Get a pattern from the pool by id.
    pub fn get_pattern(&self, id: &str) -> Option<&Pattern> {
        self.pattern_pool.get(id)
    }

    /// List all patterns in the pool.
    pub fn list_patterns(&self) -> Vec<&Pattern> {
        self.pattern_pool.values().collect()
    }

    /// Find patterns matching a given structure name.
    pub fn find_by_structure(&self, structure: &str) -> Vec<&Pattern> {
        self.pattern_pool
            .values()
            .filter(|p| p.structure == structure)
            .collect()
    }

    /// Get the number of patterns in the pool.
    pub fn pool_size(&self) -> usize {
        self.pattern_pool.len()
    }

    /// Clear all patterns from the pool.
    pub fn clear(&mut self) {
        self.pattern_pool.clear();
    }

    /// Pattern scoring: frequency * success_rate * generality.
    ///
    /// Higher scores indicate broadly applicable, frequently used, successful patterns.
    pub fn score_pattern(pattern: &Pattern, num_domains: usize) -> f64 {
        let frequency = (pattern.usage_count as f64 + 1.0).ln();
        let generality = if num_domains > 0 {
            pattern.domains.len() as f64 / num_domains as f64
        } else {
            1.0
        };
        frequency * pattern.score * generality
    }

    /// Rank all patterns in the pool by their composite score.
    pub fn rank_patterns(&self, num_domains: usize) -> Vec<(&String, f64)> {
        let mut ranked: Vec<_> = self
            .pattern_pool
            .iter()
            .map(|(k, p)| (k, Self::score_pattern(p, num_domains)))
            .collect();
        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked
    }

    /// Cross-pollinate: import patterns from another agent's pollination engine.
    pub fn cross_pollinate(&mut self, other: &CrossAgentPollination) -> Result<usize> {
        let mut imported = 0;
        for pattern in other.pattern_pool.values() {
            let mut cloned = pattern.clone();
            cloned.id = Uuid::new_v4(); // Reassign ID to avoid collisions
            let key = cloned.id.to_string();
            self.pattern_pool.insert(key, cloned);
            imported += 1;
        }
        info!(imported = %imported, "Cross-pollinated patterns from another agent");
        Ok(imported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_prompt(system: &str, template: &str) -> Prompt {
        Prompt {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: system.to_string(),
            user_template: template.to_string(),
            required_vars: vec![],
            domain: Domain::Coding,
            tags: vec![],
            target_roles: vec![],
            metadata: PromptMeta::default(),
            metrics: PromptMetrics {
                usage_count: 50,
                success_rate: 0.9,
                avg_tokens: 300,
                avg_latency_ms: 100,
                last_used: Some(Utc::now()),
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
            author: AgentIdentity {
                id: Uuid::new_v4(),
                name: "test".to_string(),
                capabilities: Default::default(),
                token_hash: "".to_string(),
                specialization_score: 0.5,
            },
            deleted_at: None,
            generation_params: None,
            locale: None,
            multimodal: None,
        }
    }

    #[test]
    fn test_extract_step_by_step_pattern() {
        let prompt = make_prompt("Follow these steps: 1. Analyze 2. Plan 3. Execute", "Help me.");
        let patterns = CrossAgentPollination::extract_patterns(&prompt);
        assert!(
            patterns.iter().any(|p| p.structure == "step-by-step"),
            "Should detect step-by-step pattern"
        );
    }

    #[test]
    fn test_extract_few_shot_pattern() {
        let prompt = make_prompt("Here are some examples: Example 1...", "Help me.");
        let patterns = CrossAgentPollination::extract_patterns(&prompt);
        assert!(
            patterns.iter().any(|p| p.structure == "few-shot"),
            "Should detect few-shot pattern"
        );
    }

    #[test]
    fn test_extract_chain_of_thought_pattern() {
        let prompt = make_prompt("Think step by step and reason about this.", "Help me.");
        let patterns = CrossAgentPollination::extract_patterns(&prompt);
        assert!(
            patterns.iter().any(|p| p.structure == "chain-of-thought"),
            "Should detect chain-of-thought pattern"
        );
    }

    #[test]
    fn test_share_and_retrieve_pattern() {
        let mut engine = CrossAgentPollination::new();
        let pattern = Pattern {
            id: Uuid::new_v4(),
            structure: "step-by-step".to_string(),
            domains: vec![Domain::Coding],
            score: 0.95,
            usage_count: 100,
            agent_id: Uuid::new_v4(),
            example_snippet: "Step 1...".to_string(),
        };

        let id = pattern.id.to_string();
        engine.share_pattern(pattern).unwrap();
        assert_eq!(engine.pool_size(), 1);
        assert!(engine.get_pattern(&id).is_some());
    }

    #[test]
    fn test_score_pattern() {
        let pattern = Pattern {
            id: Uuid::new_v4(),
            structure: "step-by-step".to_string(),
            domains: vec![Domain::Coding, Domain::Analysis],
            score: 0.9,
            usage_count: 100,
            agent_id: Uuid::new_v4(),
            example_snippet: "...".to_string(),
        };

        let score = CrossAgentPollination::score_pattern(&pattern, 5);
        assert!(score > 0.0, "Score should be positive");
    }

    #[test]
    fn test_score_pattern_zero_domains() {
        let pattern = Pattern {
            id: Uuid::new_v4(),
            structure: "test".to_string(),
            domains: vec![],
            score: 0.5,
            usage_count: 10,
            agent_id: Uuid::new_v4(),
            example_snippet: "...".to_string(),
        };

        let score = CrossAgentPollination::score_pattern(&pattern, 0);
        assert!(score > 0.0, "Score with zero domains should use generality=1.0");
    }

    #[test]
    fn test_find_by_structure() {
        let mut engine = CrossAgentPollination::new();
        let p1 = Pattern {
            id: Uuid::new_v4(),
            structure: "step-by-step".to_string(),
            domains: vec![Domain::Coding],
            score: 0.9,
            usage_count: 10,
            agent_id: Uuid::new_v4(),
            example_snippet: "...".to_string(),
        };
        let p2 = Pattern {
            id: Uuid::new_v4(),
            structure: "few-shot".to_string(),
            domains: vec![Domain::Writing],
            score: 0.8,
            usage_count: 5,
            agent_id: Uuid::new_v4(),
            example_snippet: "...".to_string(),
        };
        engine.share_pattern(p1).unwrap();
        engine.share_pattern(p2).unwrap();

        let found = engine.find_by_structure("step-by-step");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn test_rank_patterns() {
        let mut engine = CrossAgentPollination::new();
        let p1 = Pattern {
            id: Uuid::new_v4(),
            structure: "high-score".to_string(),
            domains: vec![Domain::Coding, Domain::Analysis],
            score: 0.99,
            usage_count: 1000,
            agent_id: Uuid::new_v4(),
            example_snippet: "...".to_string(),
        };
        let p2 = Pattern {
            id: Uuid::new_v4(),
            structure: "low-score".to_string(),
            domains: vec![Domain::Coding],
            score: 0.1,
            usage_count: 1,
            agent_id: Uuid::new_v4(),
            example_snippet: "...".to_string(),
        };
        engine.share_pattern(p1).unwrap();
        engine.share_pattern(p2).unwrap();

        let ranked = engine.rank_patterns(5);
        assert_eq!(ranked.len(), 2);
        assert!(ranked[0].1 > ranked[1].1, "Higher usage+score should rank first");
    }

    #[test]
    fn test_cross_pollinate() {
        let mut engine_a = CrossAgentPollination::new();
        let mut engine_b = CrossAgentPollination::new();

        let p = Pattern {
            id: Uuid::new_v4(),
            structure: "test".to_string(),
            domains: vec![Domain::Coding],
            score: 0.8,
            usage_count: 5,
            agent_id: Uuid::new_v4(),
            example_snippet: "...".to_string(),
        };
        engine_b.share_pattern(p).unwrap();
        assert_eq!(engine_b.pool_size(), 1);

        let imported = engine_a.cross_pollinate(&engine_b).unwrap();
        assert_eq!(imported, 1);
        assert_eq!(engine_a.pool_size(), 1);
    }

    #[test]
    fn test_clear_pool() {
        let mut engine = CrossAgentPollination::new();
        let p = Pattern::default();
        engine.share_pattern(p).unwrap();
        assert_eq!(engine.pool_size(), 1);
        engine.clear();
        assert_eq!(engine.pool_size(), 0);
    }
}
