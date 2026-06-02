use prompt_hub::rollback::SafeDeployer;
use prompt_hub::models::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let deployer = SafeDeployer::default();
    let artifact = Artifact::Prompt {
        system: "You are a helper".to_string(),
        user: "Hello".to_string(),
    };
    let result = deployer.deploy_with_rollback(&artifact, true).await?;
    println!("Safe Deploy Demo: {:?}", result);
    Ok(())
}
