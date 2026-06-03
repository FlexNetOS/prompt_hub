#![forbid(unsafe_code)]
use anyhow::Result;
use prompt_hub::{HubConfig, hub::PromptHub, models::*};
use std::path::Path;
use tracing::info;

pub async fn run(
    domain: Option<Domain>,
    status: Option<Status>,
    page: Option<usize>,
    per_page: Option<usize>,
) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;

    let pagination = Pagination {
        page: page.unwrap_or(1),
        per_page: per_page.unwrap_or(20),
    };

    let results = hub.list(pagination).await?;

    let filtered: Vec<_> = results
        .items
        .into_iter()
        .filter(|p| domain.as_ref().map(|d| p.domain == *d).unwrap_or(true))
        .filter(|p| status.as_ref().map(|s| p.status == *s).unwrap_or(true))
        .collect();

    info!(
        "Listed {} prompts (domain={:?}, status={:?})",
        filtered.len(),
        domain,
        status
    );

    if filtered.is_empty() {
        println!(
            "No prompts found (domain={:?}, status={:?})",
            domain, status
        );
    } else {
        println!("Found {} prompts:", filtered.len());
        for p in &filtered {
            println!(
                "  - {} (v{}): {:?} [{:?}] - tags: {:?}",
                p.name, p.version, p.domain, p.status, p.tags
            );
        }
    }
    Ok(())
}
