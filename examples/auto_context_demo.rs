use prompt_hub::context_gatherer::ContextGatherer;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = ContextGatherer::gather(Path::new(".")).await?;
    println!("Auto-Context Demo:");
    println!("  Detected language: {}", ctx.language);
    println!("  Detected framework: {}", ctx.framework);
    Ok(())
}
