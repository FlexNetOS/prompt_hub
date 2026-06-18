#![forbid(unsafe_code)]
use anyhow::Result;
use prompt_hub::{HubConfig, hub::PromptHub, models::Intent};
use std::path::Path;

pub async fn run(request: &str) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;
    let intent = Intent {
        raw_text: request.to_string(),
        ..Default::default()
    };
    let ctx = hub.gather_context(Path::new(".")).await?;
    let estimate = hub.estimate_cost(&intent, &ctx).await?;
    println!("Cost estimate for '{}'", request);
    println!(
        "  Tokens: {} in / {} out",
        estimate.tokens_input, estimate.tokens_output
    );
    println!("  Estimated cost: ${:.4}", estimate.cost_usd);
    println!("  Time estimate: {}s", estimate.time_seconds);
    println!("  Confidence: {:.0}%", estimate.confidence * 100.0);
    Ok(())
}
