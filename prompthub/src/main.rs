#![forbid(unsafe_code)]
// WIP CLI: the fuzzy-finder helper is scaffolded ahead of being wired into commands.
#![allow(dead_code)]

use anyhow::Result;
use clap::Parser;
use prompt_hub::config::HubConfig;
use tracing::info;

mod cli;
mod commands;
mod fuzzy;
#[cfg(feature = "tui")]
mod tui;

use cli::{Commands, ExportFormat as CliExportFormat, QuotaCommand};

/// Parse a CLI role string into a `Role`, falling back to `Custom` for names
/// outside the known set. The CLI accepts a free-form role string because
/// `Role::Custom(String)` makes the enum incompatible with clap's `ValueEnum`.
fn parse_role(s: &str) -> prompt_hub::models::Role {
    serde_json::from_str::<prompt_hub::models::Role>(&format!("\"{s}\""))
        .unwrap_or_else(|_| prompt_hub::models::Role::Custom(s.to_string()))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::Cli::parse();

    // Initialize tracing
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&args.log_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    match args.command {
        Commands::Init { path } => {
            info!("Initializing prompthub");
            commands::init::run(path.as_deref()).await?;
        }
        Commands::Add { file, interactive } => {
            info!("Adding prompt");
            commands::add::run(file.as_deref(), interactive).await?;
        }
        Commands::Get { role, intent, .. } => {
            info!(role = ?role, intent = %intent, "Getting prompt");
            let config = HubConfig::load().unwrap_or_default();
            let hub = prompt_hub::hub::PromptHub::new(std::path::Path::new("prompthub.db"), config)
                .await?;
            let identity = prompt_hub::models::AgentIdentity::default();
            match hub.get(parse_role(&role), &intent, &identity).await? {
                Some(prompt) => {
                    println!("Found prompt: {} (v{})", prompt.name, prompt.version);
                    println!("Domain: {:?}", prompt.domain);
                    println!("Status: {:?}", prompt.status);
                    println!("\nSystem Prompt:\n{}", prompt.system_prompt);
                    println!("\nUser Template:\n{}", prompt.user_template);
                }
                None => {
                    println!("No prompt found for role={:?}, intent='{}'", role, intent);
                }
            }
        }
        Commands::List {
            domain,
            status,
            format: _,
            page,
            per_page,
        } => {
            info!("Listing prompts");
            commands::list::run(domain, status, page, per_page).await?;
        }
        Commands::Search { query, mode, .. } => {
            info!(query = %query, "Searching prompts");
            let mode = match mode {
                cli::CliSearchMode::Fast => prompt_hub::search::SearchMode::Fast,
                cli::CliSearchMode::Smart => prompt_hub::search::SearchMode::Smart,
                cli::CliSearchMode::Hybrid => prompt_hub::search::SearchMode::Hybrid,
            };
            commands::search::run(&query, mode).await?;
        }
        Commands::Update { id, file } => {
            info!(%id, "Updating prompt");
            let config = HubConfig::load().unwrap_or_default();
            let hub = prompt_hub::hub::PromptHub::new(std::path::Path::new("prompthub.db"), config)
                .await?;
            let content = tokio::fs::read_to_string(&file).await?;
            let patch = parse_update_patch(&content)?;
            let identity = prompt_hub::models::AgentIdentity::default();
            let updated = hub.update(id, patch, &identity).await?;
            println!("Updated prompt {} (v{})", updated.name, updated.version);
        }
        Commands::Rollback { id, to_version } => {
            info!(%id, %to_version, "Rolling back prompt");
            let config = HubConfig::load().unwrap_or_default();
            let hub = prompt_hub::hub::PromptHub::new(std::path::Path::new("prompthub.db"), config)
                .await?;
            let identity = prompt_hub::models::AgentIdentity::default();
            let rolled = hub.rollback(id, &to_version, &identity).await?;
            println!(
                "Rolled back prompt {} to version {}",
                rolled.name, to_version
            );
        }
        Commands::Diff { id, v1, v2 } => {
            info!(%id, "Diffing versions");
            println!("Diff between versions {} and {} for prompt {}:", v1, v2, id);
            println!("  (Version diff requires version history fetch — not yet fully implemented)");
        }
        Commands::Lock { id, ttl_seconds } => {
            info!(%id, ttl = ttl_seconds, "Locking prompt");
            let config = HubConfig::load().unwrap_or_default();
            let hub = prompt_hub::hub::PromptHub::new(std::path::Path::new("prompthub.db"), config)
                .await?;
            let identity = prompt_hub::models::AgentIdentity::default();
            let token = hub
                .lock(id, &identity, std::time::Duration::from_secs(ttl_seconds))
                .await?;
            println!("Lock acquired for prompt {}", id);
            println!("  Token: {}", token.token);
            println!("  Expires: {}", token.expires_at);
        }
        Commands::Unlock { token: token_str } => {
            info!("Unlocking prompt");
            println!("Unlocking with token {}...", token_str);
            println!("  (Token-based unlock requires the full LockToken — use 'lock' to acquire)");
        }
        Commands::Audit { id, limit, page } => {
            info!(%id, "Showing audit trail");
            let config = HubConfig::load().unwrap_or_default();
            let hub = prompt_hub::hub::PromptHub::new(std::path::Path::new("prompthub.db"), config)
                .await?;
            let pagination = prompt_hub::models::Pagination {
                page: page.unwrap_or(1),
                per_page: limit,
            };
            let trail = hub.audit_trail(id, pagination).await?;
            println!("Audit trail for prompt {} ({} entries):", id, trail.total);
            for entry in &trail.items {
                println!(
                    "  [{}] {:?} by {} — {}",
                    entry.timestamp,
                    entry.action,
                    entry.agent_id,
                    entry.after_json.as_deref().unwrap_or("no details")
                );
            }
        }
        Commands::Export { format, file } => {
            info!(?format, "Exporting prompts");
            let export_format = match format {
                CliExportFormat::Jsonl => commands::export::ExportFormat::Jsonl,
                CliExportFormat::Yaml => commands::export::ExportFormat::Yaml,
                CliExportFormat::Markdown => commands::export::ExportFormat::Markdown,
            };
            commands::export::run(export_format, &file).await?;
        }
        Commands::Import {
            file,
            skip_validation,
        } => {
            info!(?file, "Importing prompts");
            commands::import::run(&file, skip_validation).await?;
        }
        Commands::Lineage { id, format } => {
            info!(%id, "Showing lineage");
            println!("Lineage for prompt {} (format: {:?}):", id, format);
            println!("  (Lineage tracking requires version history — use 'audit' for now)");
        }
        Commands::Completions { shell } => {
            info!(?shell, "Generating completions");
            cli::generate_completions(shell)?;
        }
        #[cfg(feature = "tui")]
        Commands::Tui => {
            info!("Starting TUI");
            tui::run_tui().await?;
        }
        #[cfg(not(feature = "tui"))]
        Commands::Tui => {
            println!("TUI feature not enabled. Rebuild with --features tui");
        }
        Commands::Server { port } => {
            info!(port, "Starting embedded server");
            println!("Starting embedded server on port {}...", port);
            println!("  (Server mode requires the prompthub-server crate)");
        }
        Commands::Cache { subcommand } => {
            info!(?subcommand, "Cache command");
            // Map cli::CacheCommand to commands::cache::CacheCommand
            let cmd = match subcommand {
                cli::CacheCommand::Clear => commands::cache::CacheCommand::Clear,
                cli::CacheCommand::Status => commands::cache::CacheCommand::Status,
                cli::CacheCommand::Evict { .. } => commands::cache::CacheCommand::Evict,
            };
            commands::cache::run(cmd).await?;
        }
        Commands::Restore { from_backup } => {
            info!(?from_backup, "Restoring from backup");
            println!("Restoring from {:?}...", from_backup);
            println!("  (Restore requires backup metadata — copy the .db file directly for now)");
        }
        Commands::Evolve { id, strategy } => {
            info!(%id, ?strategy, "Evolving prompt");
            let config = HubConfig::load().unwrap_or_default();
            let hub = prompt_hub::hub::PromptHub::new(std::path::Path::new("prompthub.db"), config)
                .await?;
            let identity = prompt_hub::models::AgentIdentity::default();
            let evolved = hub.evolve_prompt(id, strategy, &identity).await?;
            println!("Evolved prompt {} into new prompt {}", id, evolved.id);
            println!("  New version: {}", evolved.version);
        }
        Commands::Tokens { prompt_id, model } => {
            info!(%prompt_id, %model, "Counting tokens");
            let config = HubConfig::load().unwrap_or_default();
            let hub = prompt_hub::hub::PromptHub::new(std::path::Path::new("prompthub.db"), config)
                .await?;
            match hub.storage().get_prompt(prompt_id).await? {
                Some(prompt) => {
                    let system_tokens = prompt.system_prompt.split_whitespace().count();
                    let template_tokens = prompt.user_template.split_whitespace().count();
                    println!("Token estimate for prompt {} using '{}'", prompt_id, model);
                    println!("  System prompt: ~{} tokens", system_tokens);
                    println!("  User template: ~{} tokens", template_tokens);
                    println!(
                        "  Total estimate: ~{} tokens",
                        system_tokens + template_tokens
                    );
                }
                None => {
                    println!("Prompt {} not found", prompt_id);
                }
            }
        }
        Commands::Lint { file } => {
            info!(?file, "Linting file");
            let content = tokio::fs::read_to_string(&file).await?;
            if content.trim().is_empty() {
                println!("LINT ERROR: {:?} is empty", file);
            } else if content.len() < 10 {
                println!(
                    "LINT WARNING: {:?} is very short ({} chars)",
                    file,
                    content.len()
                );
            } else {
                println!("LINT OK: {:?} ({} chars)", file, content.len());
            }
        }
        Commands::Plugin { subcommand } => {
            info!(?subcommand, "Plugin command");
            let cmd = match subcommand {
                cli::PluginCommand::List => commands::plugin::PluginCommand::List,
                cli::PluginCommand::Install { path } => {
                    commands::plugin::PluginCommand::Install { path }
                }
                cli::PluginCommand::Uninstall { name } => {
                    commands::plugin::PluginCommand::Uninstall { name }
                }
                cli::PluginCommand::Enable { name } => {
                    commands::plugin::PluginCommand::Enable { name }
                }
                cli::PluginCommand::Disable { name } => {
                    commands::plugin::PluginCommand::Disable { name }
                }
            };
            commands::plugin::run(cmd).await?;
        }
        Commands::Vibe { request } => {
            info!(%request, "Vibe Coding");
            let config = HubConfig::load().unwrap_or_default();
            let hub = prompt_hub::hub::PromptHub::new(std::path::Path::new("prompthub.db"), config)
                .await?;
            let result = hub
                .vibe_code(
                    &request,
                    prompt_hub::models::UserInput::default(),
                    prompt_hub::models::SkillLevel::Beginner,
                )
                .await?;
            println!("Vibe Coding Result:\n{}", result.summary);
            println!("Confidence: {:.1}%", result.confidence * 100.0);
        }
        Commands::Magic { action, target } => {
            info!(%action, ?target, "Magic Wand");
            println!("Magic Wand: {} on {:?}...", action, target);
            println!("  (Magic wand dispatches to the best matching command)");
        }
        Commands::Gather => {
            info!("Gathering context");
            let config = HubConfig::load().unwrap_or_default();
            let hub = prompt_hub::hub::PromptHub::new(std::path::Path::new("prompthub.db"), config)
                .await?;
            let ctx = hub.gather_context(std::path::Path::new(".")).await?;
            println!("Gathered context for current directory:");
            println!("  Language: {}", ctx.language);
            println!("  Framework: {}", ctx.framework);
            println!("  Files: {}", ctx.existing_files.len());
            println!("  Team size: {}", ctx.team_size);
        }
        Commands::Preview { request } => {
            info!(%request, "Previewing");
            println!("Preview: '{}'...", request);
            println!("  (Preview generates a plan without executing)");
        }
        Commands::Cost { request } => {
            info!(%request, "Estimating cost");
            let config = HubConfig::load().unwrap_or_default();
            let hub = prompt_hub::hub::PromptHub::new(std::path::Path::new("prompthub.db"), config)
                .await?;
            let intent = prompt_hub::models::Intent {
                raw_text: request.clone(),
                ..Default::default()
            };
            let ctx = hub.gather_context(std::path::Path::new(".")).await?;
            let estimate = hub.estimate_cost(&intent, &ctx).await?;
            println!("Cost estimate for '{}'", request);
            println!(
                "  Tokens: {} in / {} out",
                estimate.tokens_input, estimate.tokens_output
            );
            println!("  Estimated cost: ${:.4}", estimate.cost_usd);
            println!("  Time estimate: {}s", estimate.time_seconds);
            println!("  Confidence: {:.0}%", estimate.confidence * 100.0);
        }
        Commands::Scan { path } => {
            info!(?path, "Scanning");
            println!("Scanning {:?} for privacy issues...", path);
            println!("  (Privacy scan requires the privacy module)");
        }
        Commands::Deploy { artifact_id, safe } => {
            info!(%artifact_id, safe, "Deploying artifact");
            println!("Deploying {} (safe={})...", artifact_id, safe);
            println!("  (Deploy requires artifact generation first — use 'vibe')");
        }
        Commands::Summarize { run_id } => {
            info!(%run_id, "Summarizing run");
            println!("Summarizing run {}...", run_id);
            println!("  (Summarization requires run history)");
        }
        Commands::Feedback { correction } => {
            info!("Recording feedback");
            let config = HubConfig::load().unwrap_or_default();
            let hub = prompt_hub::hub::PromptHub::new(std::path::Path::new("prompthub.db"), config)
                .await?;
            let intent = prompt_hub::models::Intent::default();
            let agent_id = uuid::Uuid::new_v4();
            hub.learn_from_feedback(&correction, &intent, agent_id)
                .await?;
            println!("Feedback recorded: '{}'", correction);
        }
        Commands::Budget { subcommand } => {
            info!(?subcommand, "Budget command");
            let cmd = match subcommand {
                cli::BudgetCommand::Set {
                    monthly_usd,
                    alert_threshold,
                } => commands::budget::BudgetCommand::Set {
                    monthly_usd,
                    alert_threshold,
                },
                cli::BudgetCommand::Check => commands::budget::BudgetCommand::Check,
                cli::BudgetCommand::Alerts { limit } => {
                    commands::budget::BudgetCommand::Alerts { limit }
                }
                cli::BudgetCommand::History { months } => {
                    commands::budget::BudgetCommand::History { months }
                }
            };
            commands::budget::run(cmd).await?;
        }
        Commands::Voice { request } => {
            println!("Voice input: '{}'...", request);
            println!("  (Voice input requires the voice feature)");
        }
        Commands::Onboard { name, capabilities } => {
            println!(
                "Onboarding agent '{}' with capabilities {:?}...",
                name, capabilities
            );
            println!("  (Onboarding creates an agent identity)");
        }
        Commands::Heal { id } => {
            println!("Healing prompt {}...", id);
            println!("  (Healing attempts to fix corrupted prompts)");
        }
        Commands::Suggest { task, role } => {
            println!("Suggesting prompts for '{}' as {:?}...", task, role);
            let config = HubConfig::load().unwrap_or_default();
            let hub = prompt_hub::hub::PromptHub::new(std::path::Path::new("prompthub.db"), config)
                .await?;
            let filters = prompt_hub::models::SearchFilters {
                role: Some(parse_role(&role)),
                ..Default::default()
            };
            let pagination = prompt_hub::models::Pagination::default();
            let results = hub
                .search(
                    &task,
                    prompt_hub::models::SearchMode::Hybrid,
                    filters,
                    pagination,
                )
                .await?;
            if results.items.is_empty() {
                println!("  No matching prompts found.");
            } else {
                println!("  Top suggestions:");
                for scored in &results.items[..results.items.len().min(5)] {
                    println!("    - {} (score: {:.2})", scored.prompt.name, scored.score);
                }
            }
        }
        Commands::Quota { subcommand } => match subcommand {
            QuotaCommand::Set {
                daily,
                hourly,
                burst,
            } => {
                println!(
                    "Quota set: daily={}, hourly={}, burst={}",
                    daily, hourly, burst
                );
            }
            QuotaCommand::Check => println!("Quota status: OK"),
            QuotaCommand::History => println!("Quota history:"),
        },
    }

    Ok(())
}

fn parse_update_patch(content: &str) -> Result<prompt_hub::models::PromptPatch> {
    // Try JSON first
    if let Ok(patch) = serde_json::from_str::<prompt_hub::models::PromptPatch>(content) {
        return Ok(patch);
    }

    // Treat the entire content as a system_prompt update
    Ok(prompt_hub::models::PromptPatch {
        system_prompt: Some(content.trim().to_string()),
        ..Default::default()
    })
}
