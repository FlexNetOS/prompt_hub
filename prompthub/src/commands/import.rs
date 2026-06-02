#![forbid(unsafe_code)]
use prompt_hub::{HubConfig, hub::PromptHub, models::*};
use anyhow::{Context, Result};
use std::path::Path;
use tracing::{info, warn};

pub async fn run(file: &Path, skip_validation: bool) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;

    info!("Importing prompts from {:?} (skip_validation={})", file, skip_validation);
    println!("Importing from {:?}...", file);

    let content = tokio::fs::read_to_string(file)
        .await
        .with_context(|| format!("Failed to read import file: {:?}", file))?;

    // Detect format and parse
    let prompts = if file.extension().map(|e| e == "yaml" || e == "yml").unwrap_or(false) {
        parse_yaml(&content)?
    } else if file.extension().map(|e| e == "jsonl").unwrap_or(false) {
        parse_jsonl(&content)?
    } else {
        // Auto-detect: try JSONL first, then YAML
        parse_jsonl(&content)
            .or_else(|_| parse_yaml(&content))
            .with_context(|| "Could not parse file as JSONL or YAML")?
    };

    if prompts.is_empty() {
        println!("No prompts found in {:?}", file);
        return Ok(());
    }

    let mut imported = 0;
    let mut failed = 0;
    let identity = AgentIdentity::default();

    for prompt in prompts {
        if !skip_validation {
            if let Err(e) = validate_prompt(&prompt) {
                warn!("Skipping invalid prompt '{}': {}", prompt.name, e);
                failed += 1;
                continue;
            }
        }

        match hub.register(prompt.clone(), &identity).await {
            Ok(id) => {
                info!("Imported prompt '{}' as {}", prompt.name, id);
                imported += 1;
            }
            Err(e) => {
                warn!("Failed to register prompt '{}': {}", prompt.name, e);
                failed += 1;
            }
        }
    }

    println!(
        "Import complete: {} imported, {} failed, {} total",
        imported,
        failed,
        imported + failed
    );
    info!(
        "Import from {:?} complete: {} imported, {} failed",
        file,
        imported,
        failed
    );
    Ok(())
}

fn parse_jsonl(content: &str) -> Result<Vec<Prompt>> {
    let mut prompts = Vec::new();
    for (line_no, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let prompt: Prompt = serde_json::from_str(line)
            .with_context(|| format!("Invalid JSON on line {}", line_no + 1))?;
        prompts.push(prompt);
    }
    Ok(prompts)
}

fn parse_yaml(content: &str) -> Result<Vec<Prompt>> {
    // Try as a sequence first
    let prompts: Vec<Prompt> =
        serde_yaml::from_str(content).context("Invalid YAML prompt array")?;
    Ok(prompts)
}

fn validate_prompt(prompt: &Prompt) -> Result<()> {
    if prompt.name.trim().is_empty() {
        anyhow::bail!("Prompt name is empty");
    }
    if prompt.system_prompt.trim().is_empty() {
        anyhow::bail!("System prompt is empty");
    }
    Ok(())
}
