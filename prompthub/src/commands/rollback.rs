#![forbid(unsafe_code)]
use anyhow::Result;
use prompt_hub::{HubConfig, hub::PromptHub};
use std::path::Path;
use uuid::Uuid;

pub async fn run(id: Uuid, to_version: &str) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;
    let identity = crate::identity::cli_identity();
    let rolled = hub.rollback(id, to_version, &identity).await?;
    println!(
        "Rolled back prompt {} to version {}",
        rolled.name, to_version
    );
    Ok(())
}
