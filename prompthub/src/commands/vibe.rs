#![forbid(unsafe_code)]
use anyhow::Result;
use prompt_hub::{
    HubConfig,
    hub::PromptHub,
    models::{SkillLevel, UserInput},
};
use std::path::Path;

pub async fn run(request: &str) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;
    let result = hub
        .vibe_code(request, UserInput::default(), SkillLevel::Beginner)
        .await?;
    println!("Vibe Coding Result:\n{}", result.summary);
    println!("Confidence: {:.1}%", result.confidence * 100.0);
    Ok(())
}
