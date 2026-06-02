use prompt_hub::evolution::EvolutionEngine;
use prompt_hub::models::*;
use chrono::Utc;
use uuid::Uuid;

fn create_test_prompt() -> Prompt {
    Prompt {
        id: Uuid::new_v4(), name: "test".to_string(),
        version: semver::Version::new(1, 0, 0),
        status: Status::Active,
        system_prompt: "Be helpful.".to_string(),
        user_template: "Help with {{task}}.".to_string(),
        required_vars: vec!["task".to_string()],
        domain: Domain::Coding, tags: vec![], target_roles: vec![],
        metadata: PromptMeta::default(),
        metrics: PromptMetrics { usage_count: 100, success_rate: 0.85, ..Default::default() },
        created_at: Utc::now(), updated_at: Utc::now(),
        author: AgentIdentity { id: Uuid::new_v4(), name: "demo".to_string(), capabilities: Default::default(), token_hash: "".to_string(), specialization_score: 0.5 },
        deleted_at: None, generation_params: None, locale: None, multimodal: None,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let parent_a = create_test_prompt();
    let parent_b = create_test_prompt();
    let child = EvolutionEngine::crossover(&parent_a, &parent_b)?;
    let fitness = EvolutionEngine::fitness(&child);
    println!("Evolution Demo: child fitness = {:.4}", fitness);
    Ok(())
}
