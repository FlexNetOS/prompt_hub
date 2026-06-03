#![forbid(unsafe_code)]
#![cfg(feature = "tui")]

use anyhow::Result;
use tracing::info;

/// Run the TUI interface
pub async fn run_tui() -> Result<()> {
    info!("Starting TUI interface");
    println!(
        "TUI mode activated (feature stub — full implementation requires ratatui integration)"
    );
    Ok(())
}
