#![forbid(unsafe_code)]
use anyhow::Result;
use prompt_hub::{HubConfig, hub::PromptHub, models::EvolutionStrategy};
use std::path::Path;
use uuid::Uuid;

pub async fn run(id: Uuid, strategy: EvolutionStrategy) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;
    let identity = crate::identity::cli_identity();
    let evolved = hub.evolve_prompt(id, strategy, &identity).await?;
    println!("Evolved prompt {} into new prompt {}", id, evolved.id);
    println!("  New version: {}", evolved.version);
    Ok(())
}
