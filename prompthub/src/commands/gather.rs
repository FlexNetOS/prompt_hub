#![forbid(unsafe_code)]
use anyhow::Result;
use prompt_hub::{HubConfig, hub::PromptHub};
use std::path::Path;

pub async fn run() -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;
    let ctx = hub.gather_context(Path::new(".")).await?;
    println!("Gathered context for current directory:");
    println!("  Language: {}", ctx.language);
    println!("  Framework: {}", ctx.framework);
    println!("  Files: {}", ctx.existing_files.len());
    println!("  Team size: {}", ctx.team_size);
    Ok(())
}
