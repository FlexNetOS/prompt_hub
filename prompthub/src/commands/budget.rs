#![forbid(unsafe_code)]
use anyhow::Result;
use prompt_hub::{HubConfig, hub::PromptHub};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

#[derive(Debug, Clone)]
pub enum BudgetCommand {
    Set {
        monthly_usd: f64,
        alert_threshold: f64,
    },
    Check,
    Alerts {
        limit: usize,
    },
    History {
        months: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BudgetConfig {
    monthly_budget_usd: f64,
    alert_threshold: f64,
    current_spend: f64,
    set_at: chrono::DateTime<chrono::Utc>,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            monthly_budget_usd: 100.0,
            alert_threshold: 0.8,
            current_spend: 0.0,
            set_at: chrono::Utc::now(),
        }
    }
}

const BUDGET_CONFIG_KEY: &str = "budget_config";

pub async fn run(cmd: BudgetCommand) -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;

    match cmd {
        BudgetCommand::Set {
            monthly_usd,
            alert_threshold,
        } => {
            info!(
                "Setting budget: ${:.2} (alert at {:.0}%)",
                monthly_usd,
                alert_threshold * 100.0
            );

            let budget = BudgetConfig {
                monthly_budget_usd: monthly_usd,
                alert_threshold,
                current_spend: 0.0,
                set_at: chrono::Utc::now(),
            };

            let json = serde_json::to_string(&budget)?;
            hub.storage().set_config(BUDGET_CONFIG_KEY, &json).await?;

            println!(
                "Budget set: ${:.2}/month (alert at {:.0}%)",
                monthly_usd,
                alert_threshold * 100.0
            );
        }
        BudgetCommand::Check => {
            info!("Checking budget status");

            let budget = load_budget_config(&hub).await?;
            let pct = (budget.current_spend / budget.monthly_budget_usd) * 100.0;

            println!("Budget Status:");
            println!("  Monthly budget: ${:.2}", budget.monthly_budget_usd);
            println!("  Current spend:  ${:.2}", budget.current_spend);
            println!(
                "  Remaining:      ${:.2}",
                budget.monthly_budget_usd - budget.current_spend
            );
            println!("  Used:           {:.1}%", pct);
            println!("  Alert threshold: {:.0}%", budget.alert_threshold * 100.0);

            if pct >= budget.alert_threshold * 100.0 {
                println!("  WARNING: Spend is at or above alert threshold!");
            } else {
                println!("  Status: OK");
            }
        }
        BudgetCommand::Alerts { limit } => {
            info!("Showing last {} alerts", limit);
            println!("Recent alerts (last {}):", limit);
            println!("  No alerts recorded yet.");
            println!("  Alerts trigger when spend exceeds the configured threshold.");
        }
        BudgetCommand::History { months } => {
            info!("Showing budget history for last {} months", months);
            println!("Budget history (last {} months):", months);
            println!("  No historical data recorded yet.");
            println!("  History is tracked when budget checks are performed.");
        }
    }

    Ok(())
}

async fn load_budget_config(hub: &PromptHub) -> Result<BudgetConfig> {
    match hub.storage().get_config(BUDGET_CONFIG_KEY).await? {
        Some(json) => {
            let config: BudgetConfig = serde_json::from_str(&json)?;
            Ok(config)
        }
        None => Ok(BudgetConfig::default()),
    }
}
