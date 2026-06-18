#![forbid(unsafe_code)]

use clap::{Command, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use prompt_hub::models::*;
use std::path::PathBuf;
use uuid::Uuid;

/// CLI-local mirror of `prompt_hub::SearchMode` so we can derive `ValueEnum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliSearchMode {
    Fast,
    Smart,
    Hybrid,
}

#[derive(Parser, Debug)]
#[command(name = "prompthub")]
#[command(about = "CLI for managing LLM prompts")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(short, long, global = true, default_value = "info")]
    pub log_level: String,

    #[arg(short, long, global = true)]
    pub non_interactive: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize a new prompt hub
    Init {
        path: Option<PathBuf>,
        /// Seed the store with the built-in base role templates (idempotent)
        #[arg(long)]
        seed: bool,
    },

    /// Add a new prompt
    Add {
        file: Option<PathBuf>,
        #[arg(short, long)]
        interactive: bool,
    },

    /// Get a prompt for a role and intent
    Get {
        role: String,
        intent: String,
        version: Option<String>,
    },

    /// List all prompts
    List {
        domain: Option<Domain>,
        status: Option<Status>,
        #[arg(short, long)]
        format: OutputFormat,
        #[arg(short, long)]
        page: Option<usize>,
        #[arg(long)]
        per_page: Option<usize>,
    },

    /// Search prompts
    Search {
        query: String,
        #[arg(short, long)]
        mode: CliSearchMode,
        #[arg(short, long)]
        top_k: Option<usize>,
        #[arg(short, long)]
        filters: Option<String>,
        #[arg(short, long)]
        page: Option<usize>,
    },

    /// Update a prompt
    Update { id: Uuid, file: PathBuf },

    /// Rollback to a previous version
    Rollback { id: Uuid, to_version: String },

    /// Show diff between two versions
    Diff { id: Uuid, v1: String, v2: String },

    /// Lock a prompt for editing
    Lock { id: Uuid, ttl_seconds: u64 },

    /// Unlock a prompt
    Unlock { token: String },

    /// Show audit trail
    Audit {
        id: Uuid,
        #[arg(short, long)]
        limit: usize,
        #[arg(short, long)]
        page: Option<usize>,
    },

    /// Export prompts
    Export {
        #[arg(short, long)]
        format: ExportFormat,
        file: PathBuf,
    },

    /// Import prompts
    Import {
        file: PathBuf,
        #[arg(long)]
        skip_validation: bool,
    },

    /// Show prompt lineage
    Lineage {
        id: Uuid,
        #[arg(short, long)]
        format: LineageFormat,
    },

    /// Generate shell completions
    Completions { shell: Shell },

    /// Launch TUI interface
    #[cfg(feature = "tui")]
    Tui,

    /// Print Prometheus metrics in text exposition format
    #[cfg(feature = "otel")]
    Metrics,

    /// Start embedded server
    Server {
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },

    /// Cache management
    Cache {
        #[command(subcommand)]
        subcommand: CacheCommand,
    },

    /// Restore from backup
    Restore {
        #[arg(long)]
        from_backup: PathBuf,
    },

    /// Evolve a prompt
    Evolve {
        id: Uuid,
        #[arg(short, long)]
        strategy: EvolutionStrategy,
    },

    /// Count tokens
    Tokens {
        prompt_id: Uuid,
        #[arg(short, long)]
        model: String,
    },

    /// Lint a prompt file
    Lint { file: PathBuf },

    /// Plugin management
    Plugin {
        #[command(subcommand)]
        subcommand: PluginCommand,
    },

    /// Vibe Coding mode
    Vibe { request: String },

    /// Magic Wand
    Magic {
        action: String,
        #[arg(short, long)]
        target: MagicTarget,
    },

    /// Gather context from current directory
    Gather,

    /// Preview before building
    Preview { request: String },

    /// Estimate cost
    Cost { request: String },

    /// Scan for privacy issues
    Scan { path: PathBuf },

    /// Deploy artifact
    Deploy {
        artifact_id: Uuid,
        #[arg(short, long)]
        safe: bool,
    },

    /// Summarize a run
    Summarize { run_id: Uuid },

    /// Submit feedback
    Feedback { correction: String },

    /// Junie specific commands
    Junie {
        #[command(subcommand)]
        subcommand: JunieCommand,
    },

    /// Budget management
    Budget {
        #[command(subcommand)]
        subcommand: BudgetCommand,
    },

    /// Voice input mode
    Voice { request: String },

    /// Onboard a new agent
    Onboard {
        name: String,
        capabilities: Vec<String>,
    },

    /// Heal a broken prompt
    Heal { id: Uuid },

    /// Suggest prompts for a task
    Suggest { task: String, role: String },

    /// Manage token quota
    Quota {
        #[command(subcommand)]
        subcommand: QuotaCommand,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum CacheCommand {
    Clear,
    Status,
    Evict { key: String },
}

#[derive(Subcommand, Debug, Clone)]
pub enum PluginCommand {
    List,
    Install { path: PathBuf },
    Uninstall { name: String },
    Enable { name: String },
    Disable { name: String },
}

#[derive(Subcommand, Debug, Clone)]
pub enum BudgetCommand {
    Set {
        monthly_usd: f64,
        #[arg(short, long)]
        alert_threshold: f64,
    },
    Check,
    Alerts {
        #[arg(short, long)]
        limit: usize,
    },
    History {
        #[arg(short, long)]
        months: usize,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum QuotaCommand {
    Set { daily: u32, hourly: u32, burst: u32 },
    Check,
    History,
}

#[derive(Debug, Subcommand, Clone)]
pub enum JunieCommand {
    /// Check Junie's health and status
    Status,
    /// Chat with Junie
    Chat { message: String },
    /// Ask Junie to perform a task
    Task { request: String },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LineageFormat {
    Text,
    Json,
    Dot,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ExportFormat {
    Jsonl,
    Yaml,
    Markdown,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MagicTarget {
    File,
    Prompt,
    Project,
}

/// Generate shell completions for the given shell.
pub fn generate_completions(shell: Shell) -> anyhow::Result<()> {
    let mut cmd: Command = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_init() {
        let args = Cli::parse_from(["prompthub", "init"]);
        assert!(matches!(args.command, Commands::Init { seed: false, .. }));
    }

    #[test]
    fn test_cli_parse_init_seed() {
        let args = Cli::parse_from(["prompthub", "init", "--seed"]);
        assert!(matches!(args.command, Commands::Init { seed: true, .. }));
    }

    #[test]
    #[cfg(feature = "otel")]
    fn test_cli_parse_metrics() {
        let args = Cli::parse_from(["prompthub", "metrics"]);
        assert!(matches!(args.command, Commands::Metrics));
    }

    #[test]
    fn test_cli_parse_search() {
        let args = Cli::parse_from(["prompthub", "search", "hello", "--mode", "hybrid"]);
        match args.command {
            Commands::Search { query, mode, .. } => {
                assert_eq!(query, "hello");
                assert_eq!(mode, CliSearchMode::Hybrid);
            }
            _ => panic!("Expected Search command"),
        }
    }

    #[test]
    fn test_output_format_variants() {
        assert_eq!(format!("{:?}", OutputFormat::Table), "Table");
        assert_eq!(format!("{:?}", OutputFormat::Json), "Json");
        assert_eq!(format!("{:?}", OutputFormat::Yaml), "Yaml");
    }

    #[test]
    fn test_magic_target_variants() {
        assert_eq!(format!("{:?}", MagicTarget::File), "File");
        assert_eq!(format!("{:?}", MagicTarget::Prompt), "Prompt");
        assert_eq!(format!("{:?}", MagicTarget::Project), "Project");
    }

    #[test]
    fn test_export_format_variants() {
        assert_eq!(format!("{:?}", ExportFormat::Jsonl), "Jsonl");
        assert_eq!(format!("{:?}", ExportFormat::Yaml), "Yaml");
        assert_eq!(format!("{:?}", ExportFormat::Markdown), "Markdown");
    }

    #[test]
    fn test_lineage_format_variants() {
        assert_eq!(format!("{:?}", LineageFormat::Text), "Text");
        assert_eq!(format!("{:?}", LineageFormat::Json), "Json");
        assert_eq!(format!("{:?}", LineageFormat::Dot), "Dot");
    }
}
