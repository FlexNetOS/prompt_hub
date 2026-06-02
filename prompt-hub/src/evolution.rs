#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::*;
use tracing::{info, instrument};
use uuid::Uuid;

/// Evolution engine using genetic algorithms for prompt improvement.
///
/// Provides crossover (combining parent prompts), mutation (random perturbation),
/// fitness scoring, and tournament selection with elitism.
#[derive(Debug, Clone, Default)]
pub struct EvolutionEngine;

impl EvolutionEngine {
    /// Crossover: combine two parent prompts into a child prompt.
    ///
    /// The child inherits fields from `parent_a` and blends the user template
    /// with content from `parent_b`. A new UUID and reset version are assigned.
    #[instrument]
    pub fn crossover(parent_a: &Prompt, parent_b: &Prompt) -> Result<Prompt> {
        let mut child = parent_a.clone();
        child.id = Uuid::new_v4();
        child.version = semver::Version::new(0, 1, 0);
        child.status = Status::Draft;

        // Blend user templates if they differ
        if parent_a.user_template != parent_b.user_template {
            child.user_template = format!(
                "{}\n\nAlso consider: {}",
                parent_a.user_template, parent_b.user_template
            );
        }

        // Blend system prompts if they differ
        if parent_a.system_prompt != parent_b.system_prompt {
            child.system_prompt = format!(
                "{}\n{}",
                parent_a.system_prompt, parent_b.system_prompt
            );
        }

        // Merge tags from both parents
        let mut merged_tags = parent_a.tags.clone();
        for tag in &parent_b.tags {
            if !merged_tags.contains(tag) {
                merged_tags.push(tag.clone());
            }
        }
        child.tags = merged_tags;

        // Reset metrics for the child
        child.metrics = PromptMetrics::default();

        info!(
            child_id = %child.id,
            parent_a_id = %parent_a.id,
            parent_b_id = %parent_b.id,
            "Crossover produced child prompt"
        );

        Ok(child)
    }

    /// Mutate: apply random perturbation to a prompt.
    ///
    /// When `mutation_rate > 0.5`, appends a best-practices directive to the
    /// system prompt. Always assigns a new id and resets to Draft status.
    #[instrument]
    pub fn mutate(prompt: &Prompt, mutation_rate: f64) -> Result<Prompt> {
        let mut mutated = prompt.clone();
        mutated.id = Uuid::new_v4();
        mutated.status = Status::Draft;

        if mutation_rate > 0.5 {
            mutated.system_prompt = format!(
                "{}\nEnsure best practices are followed.",
                mutated.system_prompt
            );
        }

        if mutation_rate > 0.8 {
            mutated.user_template = format!(
                "{}\n\nPlease be thorough and provide examples where appropriate.",
                mutated.user_template
            );
        }

        info!(
            mutated_id = %mutated.id,
            original_id = %prompt.id,
            mutation_rate = %mutation_rate,
            "Prompt mutation applied"
        );

        Ok(mutated)
    }

    /// Fitness function combining multiple metrics.
    ///
    /// Formula: `success_rate * 0.4 + usage_score * 0.3 + token_efficiency * 0.2 + recency * 0.1`
    pub fn fitness(prompt: &Prompt) -> f64 {
        let usage_score = (prompt.metrics.usage_count as f64 + 1.0).ln() / 10.0;
        let token_eff = if prompt.metrics.avg_tokens > 0 {
            1000.0 / prompt.metrics.avg_tokens as f64
        } else {
            0.5
        };

        let score = prompt.metrics.success_rate * 0.4
            + usage_score.min(1.0) * 0.3
            + token_eff.min(1.0) * 0.2
            + 0.1; // recency placeholder

        score.clamp(0.0, 1.0)
    }

    /// Selection: tournament selection with elitism.
    ///
    /// Cycles through candidates deterministically and returns
    /// the one with the highest fitness score.
    pub fn select_tournament<'a>(pool: &'a [Prompt], tournament_size: usize) -> &'a Prompt {
        if pool.is_empty() {
            panic!("Cannot select from an empty prompt pool");
        }
        if pool.len() == 1 || tournament_size == 0 {
            return &pool[0];
        }

        let actual_size = tournament_size.min(pool.len());

        // Use a simple deterministic selection by cycling through indices.
        // This avoids the `rand` dependency while still exploring the pool.
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let offset = COUNTER.fetch_add(1, Ordering::Relaxed);

        let selected: Vec<_> = (0..actual_size)
            .map(|i| &pool[(offset + i) % pool.len()])
            .collect();
        selected
            .into_iter()
            .max_by(|a, b| {
                Self::fitness(a)
                    .partial_cmp(&Self::fitness(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(&pool[0])
    }

    /// Run one generation of evolution: selection, crossover, mutation.
    ///
    /// Returns the next generation of prompts.
    pub fn evolve_generation(
        pool: &[Prompt],
        tournament_size: usize,
        mutation_rate: f64,
        offspring_count: usize,
    ) -> Result<Vec<Prompt>> {
        let mut next_gen = Vec::with_capacity(offspring_count);

        for _ in 0..offspring_count {
            let parent_a = Self::select_tournament(pool, tournament_size);
            let parent_b = Self::select_tournament(pool, tournament_size);

            let child = Self::crossover(parent_a, parent_b)?;
            let mutated = Self::mutate(&child, mutation_rate)?;
            next_gen.push(mutated);
        }

        info!(
            generation_size = %offspring_count,
            "Evolved one generation of prompts"
        );

        Ok(next_gen)
    }

    /// Elitism: select the top-k fittest prompts from the pool.
    pub fn select_elite(pool: &[Prompt], count: usize) -> Vec<Prompt> {
        let mut scored: Vec<_> = pool.iter().map(|p| (Self::fitness(p), p)).collect();
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.into_iter().take(count).map(|(_, p)| p.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_prompt() -> Prompt {
        Prompt {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: "You are a helper.".to_string(),
            user_template: "Help me.".to_string(),
            required_vars: vec![],
            domain: Domain::Coding,
            tags: vec!["test".to_string()],
            target_roles: vec![Role::Developer],
            metadata: PromptMeta::default(),
            metrics: PromptMetrics {
                usage_count: 100,
                success_rate: 0.85,
                avg_tokens: 500,
                avg_latency_ms: 120,
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

    fn test_prompt_b() -> Prompt {
        Prompt {
            id: Uuid::new_v4(),
            name: "test_b".to_string(),
            version: semver::Version::new(1, 0, 0),
            status: Status::Active,
            system_prompt: "You are an expert coder.".to_string(),
            user_template: "Write code for me.".to_string(),
            required_vars: vec![],
            domain: Domain::Coding,
            tags: vec!["code".to_string()],
            target_roles: vec![Role::Architect],
            metadata: PromptMeta::default(),
            metrics: PromptMetrics {
                usage_count: 200,
                success_rate: 0.92,
                avg_tokens: 800,
                avg_latency_ms: 200,
                last_used: Some(Utc::now()),
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
            author: AgentIdentity {
                id: Uuid::new_v4(),
                name: "test_b".to_string(),
                capabilities: Default::default(),
                token_hash: "".to_string(),
                specialization_score: 0.8,
            },
            deleted_at: None,
            generation_params: None,
            locale: None,
            multimodal: None,
        }
    }

    #[test]
    fn test_crossover() {
        let a = test_prompt();
        let b = test_prompt_b();
        let child = EvolutionEngine::crossover(&a, &b).unwrap();

        assert_ne!(child.id, a.id);
        assert_ne!(child.id, b.id);
        assert!(child.user_template.contains(&a.user_template));
        assert!(child.user_template.contains(&b.user_template));
        assert_eq!(child.status, Status::Draft);
        assert_eq!(child.version, semver::Version::new(0, 1, 0));
        assert_eq!(child.metrics, PromptMetrics::default());

        // Tags should be merged
        assert!(child.tags.contains(&"test".to_string()));
        assert!(child.tags.contains(&"code".to_string()));
    }

    #[test]
    fn test_mutate_low_rate() {
        let p = test_prompt();
        let mutated = EvolutionEngine::mutate(&p, 0.3).unwrap();

        assert_ne!(mutated.id, p.id);
        assert_eq!(mutated.status, Status::Draft);
        // At 0.3 rate, no system prompt mutation
        assert_eq!(mutated.system_prompt, p.system_prompt);
    }

    #[test]
    fn test_mutate_high_rate() {
        let p = test_prompt();
        let mutated = EvolutionEngine::mutate(&p, 0.6).unwrap();

        assert_ne!(mutated.id, p.id);
        assert!(mutated.system_prompt.contains("best practices"));
    }

    #[test]
    fn test_mutate_very_high_rate() {
        let p = test_prompt();
        let mutated = EvolutionEngine::mutate(&p, 0.9).unwrap();

        assert_ne!(mutated.id, p.id);
        assert!(mutated.system_prompt.contains("best practices"));
        assert!(mutated.user_template.contains("thorough"));
    }

    #[test]
    fn test_fitness() {
        let p = test_prompt();
        let f = EvolutionEngine::fitness(&p);
        assert!(f > 0.0 && f <= 1.0, "fitness should be in (0, 1], got {}", f);
    }

    #[test]
    fn test_fitness_zero_tokens() {
        let mut p = test_prompt();
        p.metrics.avg_tokens = 0;
        let f = EvolutionEngine::fitness(&p);
        assert!(f > 0.0 && f <= 1.0, "fitness with zero tokens should still be valid");
    }

    #[test]
    fn test_tournament_selection() {
        let pool = vec![test_prompt(), test_prompt_b()];
        let winner = EvolutionEngine::select_tournament(&pool, 2);

        // Winner should be one of the pool members
        assert!(winner.id == pool[0].id || winner.id == pool[1].id);
    }

    #[test]
    fn test_tournament_selection_single() {
        let pool = vec![test_prompt()];
        let winner = EvolutionEngine::select_tournament(&pool, 3);
        assert_eq!(winner.id, pool[0].id);
    }

    #[test]
    fn test_evolve_generation() {
        let pool = vec![test_prompt(), test_prompt_b()];
        let next_gen = EvolutionEngine::evolve_generation(&pool, 2, 0.6, 4).unwrap();
        assert_eq!(next_gen.len(), 4);

        // All children should have unique IDs
        let ids: std::collections::HashSet<_> = next_gen.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn test_select_elite() {
        let pool = vec![test_prompt(), test_prompt_b()];
        let elite = EvolutionEngine::select_elite(&pool, 1);
        assert_eq!(elite.len(), 1);
        // test_prompt_b has higher fitness (0.92 vs 0.85)
        assert_eq!(elite[0].name, "test_b");
    }

    #[test]
    fn test_crossover_same_templates() {
        let a = test_prompt();
        let b = test_prompt();
        let child = EvolutionEngine::crossover(&a, &b).unwrap();
        // When templates are identical, no "Also consider" should be added
        assert!(!child.user_template.contains("Also consider"));
    }
}
