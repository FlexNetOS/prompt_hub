use prompt_hub::hub::PromptHub;
use prompt_hub::config::HubConfig;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("PromptHub — Basic Usage Example");
    println!("================================\n");

    // Load configuration (falls back to defaults if no config file found)
    let config = HubConfig::load().unwrap_or_default();
    println!("Configuration:");
    println!("  Pool size: {}", config.max_pool_size);
    println!("  Default page size: {}", config.default_page_size);
    println!("  Embedding model: {}", config.embedding_model);
    println!("  Embedding dimension: {}", config.embedding_dimension);

    // Initialize the hub with an in-memory database
    let db_path = Path::new(":memory:");
    let hub = PromptHub::new(db_path, config).await?;

    println!("\nPromptHub initialized successfully!");
    println!("  Database: {:?}", hub.db_path());
    println!("  Initialized: {}", hub.is_initialized().await);

    // Example of how you would use the hub:
    // let prompt_id = hub.register(new_prompt, &agent_identity).await?;
    // let prompt = hub.get(Role::Developer, "error handling", &agent).await?;
    // let results = hub.search("async rust", SearchMode::Hybrid, &filters, &pagination).await?;

    println!("\nDone! The hub is ready for use.");
    Ok(())
}
