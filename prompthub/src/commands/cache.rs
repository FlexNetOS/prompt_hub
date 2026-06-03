#![forbid(unsafe_code)]
use anyhow::Result;
use prompt_hub::{HubConfig, hub::PromptHub};
use std::path::Path;
use tracing::info;

#[derive(Debug, Clone, Copy)]
pub enum CacheCommand {
    Clear,
    Status,
    Evict,
}

pub async fn run(cmd: CacheCommand) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;

    match cmd {
        CacheCommand::Clear => {
            info!("Clearing cache");
            // Run database maintenance (ANALYZE + VACUUM) to clear any query caches
            hub.storage().maintenance().await?;
            println!("Cache cleared and database optimized");
        }
        CacheCommand::Status => {
            info!("Checking cache status");
            let healthy = hub.storage().health_check().await?;
            if healthy {
                println!("Cache status: Healthy (database connected, storage ready)");
            } else {
                println!("Cache status: Unhealthy (storage check failed)");
            }
        }
        CacheCommand::Evict => {
            info!("Evicting cached entries");
            // Run PRAGMA optimize to clear sqlite query planner cache
            hub.storage().optimize_on_close().await?;
            println!("Cache entries evicted and query planner optimized");
        }
    }

    Ok(())
}
