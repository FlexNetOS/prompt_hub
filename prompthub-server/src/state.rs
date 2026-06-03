#![forbid(unsafe_code)]

use prompt_hub::config::HubConfig;
use prompt_hub::hub::PromptHub;
use std::path::Path;
use std::sync::Arc;

/// Shared application state for all route handlers.
///
/// Wrapped in an `Arc` and passed to every axum handler via the
/// [`axum::extract::State`] extractor.
#[derive(Debug)]
pub struct AppState {
    pub hub: Arc<PromptHub>,
    pub config: HubConfig,
    pub start_time: std::time::Instant,
}

impl AppState {
    /// Create a new AppState by initializing a real PromptHub instance.
    ///
    /// # Errors
    ///
    /// Returns an error if PromptHub fails to initialize (e.g. database
    /// cannot be opened or migrations fail).
    pub async fn new(
        db_path: &Path,
        config: HubConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let hub = Arc::new(PromptHub::new(db_path, config.clone()).await?);
        Ok(Self {
            hub,
            config,
            start_time: std::time::Instant::now(),
        })
    }

    /// Return the time elapsed since the server started.
    pub fn uptime(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }
}
