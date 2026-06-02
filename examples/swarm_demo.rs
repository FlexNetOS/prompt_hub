use prompt_hub::{PromptHub, HubConfig, Role, Domain};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = HubConfig::default();
    let hub = PromptHub::new(Path::new(":memory:"), config).await?;
    println!("Swarm Bundle Demo");
    let roles = vec![Role::Orchestrator, Role::Architect, Role::Implementer];
    println!("Roles: {:?}", roles);
    Ok(())
}
