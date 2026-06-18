#![forbid(unsafe_code)]
use anyhow::Result;
use prompt_hub::{HubConfig, hub::PromptHub, models::Intent};
use std::path::Path;
use uuid::Uuid;

pub async fn run(correction: &str) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;
    let intent = Intent::default();
    let agent_id = Uuid::new_v4();
    hub.learn_from_feedback(correction, &intent, agent_id)
        .await?;
    println!("Feedback recorded: '{}'", correction);
    Ok(())
}
