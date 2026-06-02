#![forbid(unsafe_code)]
use prompt_hub::{HubConfig, hub::PromptHub, models::*};
use prompt_hub::search::SearchMode;
use anyhow::Result;
use std::path::Path;
use tracing::info;

pub async fn run(query: &str, mode: SearchMode) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;
    let filters = SearchFilters::default();
    let pagination = Pagination::default();
    let results = hub.search(query, mode, filters, pagination).await?;
    info!("Found {} results for '{}'", results.total, query);
    if results.items.is_empty() {
        println!("No results found for '{}'", query);
    } else {
        println!("Found {} results for '{}':", results.total, query);
        for (i, scored) in results.items.iter().enumerate() {
            let preview: String = scored.prompt.system_prompt.chars().take(80).collect();
            println!(
                "  {}. {} (score: {:.2}) - {}",
                i + 1,
                scored.prompt.name,
                scored.score,
                preview
            );
        }
    }
    Ok(())
}
