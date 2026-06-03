#![forbid(unsafe_code)]
use anyhow::Result;
use chrono::Utc;
use prompt_hub::{HubConfig, hub::PromptHub, models::*};
use std::path::Path;
use tracing::info;
use uuid::Uuid;

pub async fn run(file: Option<&Path>, interactive: bool) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;

    let prompt = if let Some(path) = file {
        info!("Reading prompt from file: {:?}", path);
        let content = tokio::fs::read_to_string(path).await?;
        parse_prompt_from_content(&content).await?
    } else if interactive {
        info!("Interactive prompt creation");
        println!("Interactive prompt creation wizard");
        println!("(Using default prompt -- interactive input not yet implemented)");
        create_default_prompt()
    } else {
        info!("Creating default prompt");
        println!("No file specified, creating a default prompt.");
        println!("Use --interactive for guided creation or provide a file path.");
        create_default_prompt()
    };

    let identity = AgentIdentity::default();
    let id = hub.register(prompt, &identity).await?;
    info!("Registered prompt {}", id);
    println!("Registered prompt {}", id);
    Ok(())
}

fn create_default_prompt() -> Prompt {
    Prompt {
        id: Uuid::new_v4(),
        name: "new-prompt".to_string(),
        version: semver::Version::new(0, 1, 0),
        status: Status::Draft,
        system_prompt: "You are a helpful assistant.".to_string(),
        user_template: "{{input}}".to_string(),
        required_vars: vec!["input".to_string()],
        domain: Domain::General,
        tags: vec![],
        target_roles: vec![],
        metadata: PromptMeta::default(),
        metrics: PromptMetrics::default(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        author: AgentIdentity::default(),
        deleted_at: None,
        generation_params: None,
        locale: None,
        multimodal: None,
    }
}

async fn parse_prompt_from_content(content: &str) -> Result<Prompt> {
    // Try parsing as JSON first
    if let Ok(prompt) = serde_json::from_str::<Prompt>(content) {
        return Ok(prompt);
    }

    // Try parsing as YAML
    if let Ok(prompt) = serde_yaml::from_str::<Prompt>(content) {
        return Ok(prompt);
    }

    // Fall back to treating the content as a plain system prompt
    let mut prompt = create_default_prompt();
    prompt.system_prompt = content.trim().to_string();
    prompt.name = "imported-prompt".to_string();
    prompt.id = Uuid::new_v4();
    Ok(prompt)
}
