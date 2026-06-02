#![forbid(unsafe_code)]
use prompt_hub::{HubConfig, hub::PromptHub};
use std::path::Path;
use anyhow::Result;
use tracing::info;

pub async fn run(path: Option<&Path>) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let db_path = path.unwrap_or_else(|| Path::new("prompthub.db"));
    let _hub = PromptHub::new(db_path, config).await?;
    info!("PromptHub initialized at {:?}", db_path);
    println!("PromptHub initialized at {:?}", db_path);
    Ok(())
}
