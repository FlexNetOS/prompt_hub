#![forbid(unsafe_code)]
//! `prompthub metrics` — print the in-process metrics in Prometheus text
//! exposition format (v0.0.4), mirroring the server's `/metrics` route.
//!
//! Feature-gated behind `otel`, matching `prompt-hub`'s metrics surface.

use anyhow::Result;
use prompt_hub::{HubConfig, hub::PromptHub};
use std::path::Path;
use tracing::info;

/// Render the current metrics registry as Prometheus text and print it to
/// stdout. Returns an error if the exposition encoder fails.
pub async fn run() -> Result<()> {
    let config = HubConfig::load().unwrap_or_default();
    let hub = PromptHub::new(Path::new("prompthub.db"), config).await?;

    info!("Rendering Prometheus metrics exposition");

    let body = hub.metrics().prometheus_text()?;
    print!("{body}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn metrics_renders_valid_exposition() {
        // An in-memory hub starts with a zeroed registry; the encoder must
        // still emit a well-formed exposition (HELP/TYPE preamble lines).
        let hub = PromptHub::new(Path::new(":memory:"), HubConfig::default())
            .await
            .expect("hub construction");
        let body = hub
            .metrics()
            .prometheus_text()
            .expect("prometheus exposition");
        assert!(
            body.contains("# HELP") && body.contains("# TYPE"),
            "exposition missing HELP/TYPE preamble:\n{body}"
        );
    }
}
