#![forbid(unsafe_code)]
use prompt_hub::{HubConfig, hub::PromptHub, models::*};
use anyhow::Result;
use std::path::Path;
use tracing::info;

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Jsonl,
    Yaml,
    Markdown,
}

pub async fn run(format: ExportFormat, file: &Path) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;

    let pagination = Pagination {
        page: 1,
        per_page: 10_000,
    };
    let results = hub.list(pagination).await?;

    info!(
        "Exporting {} prompts to {:?} in {:?} format",
        results.total,
        file,
        format
    );

    match format {
        ExportFormat::Jsonl => export_jsonl(&results.items, file).await?,
        ExportFormat::Yaml => export_yaml(&results.items, file).await?,
        ExportFormat::Markdown => export_markdown(&results.items, file).await?,
    }

    println!(
        "Exported {} prompts to {:?} ({:?})",
        results.total,
        file,
        format
    );
    Ok(())
}

async fn export_jsonl(prompts: &[Prompt], file: &Path) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut f = tokio::fs::File::create(file).await?;
    for prompt in prompts {
        let line = serde_json::to_string(prompt)?;
        f.write_all(line.as_bytes()).await?;
        f.write_all(b"\n").await?;
    }
    f.flush().await?;
    info!("Wrote {} prompts as JSONL to {:?}", prompts.len(), file);
    Ok(())
}

async fn export_yaml(prompts: &[Prompt], file: &Path) -> Result<()> {
    let yaml = serde_yaml::to_string(prompts)?;
    tokio::fs::write(file, yaml).await?;
    info!("Wrote {} prompts as YAML to {:?}", prompts.len(), file);
    Ok(())
}

async fn export_markdown(prompts: &[Prompt], file: &Path) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut f = tokio::fs::File::create(file).await?;

    f.write_all(b"# PromptHub Export\n\n").await?;
    f.write_all(format!("Generated: {}\n\n", chrono::Utc::now().to_rfc3339()).as_bytes())
        .await?;
    f.write_all(format!("Total prompts: {}\n\n", prompts.len()).as_bytes())
        .await?;

    for (i, prompt) in prompts.iter().enumerate() {
        let md = format!(
            "## {}. {} (v{})\n\n"
            "- **ID:** {}\n"
            "- **Domain:** {:?}\n"
            "- **Status:** {:?}\n"
            "- **Tags:** {:?}\n"
            "- **Required vars:** {:?}\n"
            "- **Created:** {}\n"
            "- **Updated:** {}\n\n"
            "### System Prompt\n\n```\n{}\n```\n\n"
            "### User Template\n\n```\n{}\n```\n\n"
            "---\n\n",
            i + 1,
            prompt.name,
            prompt.version,
            prompt.id,
            prompt.domain,
            prompt.status,
            prompt.tags,
            prompt.required_vars,
            prompt.created_at,
            prompt.updated_at,
            prompt.system_prompt,
            prompt.user_template
        );
        f.write_all(md.as_bytes()).await?;
    }

    f.flush().await?;
    info!(
        "Wrote {} prompts as Markdown to {:?}",
        prompts.len(),
        file
    );
    Ok(())
}
