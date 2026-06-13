#![forbid(unsafe_code)]
use anyhow::Result;
use prompt_hub::models::AgentIdentity;
use prompt_hub::{HubConfig, hub::PromptHub};
use std::path::Path;
use tracing::info;

pub async fn run(path: Option<&Path>, seed: bool) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let db_path = path.unwrap_or_else(|| Path::new("prompthub.db"));
    let hub = PromptHub::new(db_path, config).await?;
    info!("PromptHub initialized at {:?}", db_path);
    println!("PromptHub initialized at {:?}", db_path);

    if seed {
        // The CLI acts as the trusted local operator (Read+Write+Admin), which
        // seed_defaults requires (Write).
        let operator = AgentIdentity::local_operator("prompthub-cli");
        let inserted = hub.seed_defaults(&operator).await?;
        info!(inserted, "Seeded base templates");
        println!("Seeded {inserted} base template(s)");
    }
    Ok(())
}
