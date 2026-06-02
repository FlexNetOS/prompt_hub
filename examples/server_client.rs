use prompt_hub::models::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Server Client Demo");
    println!("This example demonstrates HTTP API usage:");
    println!("  GET /api/v1/prompts          -> List prompts");
    println!("  POST /api/v1/prompts         -> Register prompt");
    println!("  GET /api/v1/prompts/search   -> Search prompts");
    println!("  GET /health                  -> Health check");
    Ok(())
}
