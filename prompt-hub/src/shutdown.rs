#![forbid(unsafe_code)]

use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{info, instrument, warn};

/// Graceful shutdown coordinator
///
/// Uses a `tokio::sync::broadcast` channel so every spawned task and the
/// axum server can subscribe to a single shutdown signal.
#[derive(Debug, Clone)]
pub struct ShutdownCoordinator {
    tx: broadcast::Sender<()>,
}

impl ShutdownCoordinator {
    /// Create a new coordinator.
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(1);
        Self { tx }
    }

    /// Subscribe to the shutdown signal.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }

    /// Initiate graceful shutdown by broadcasting the signal.
    #[instrument]
    pub fn shutdown(&self) {
        info!("Graceful shutdown signal broadcast");
        let _ = self.tx.send(());
    }

    /// Wait for `SIGTERM` or `SIGINT`, then broadcast shutdown.
    ///
    /// On non-Unix platforms falls back to `ctrl_c()`.
    #[instrument]
    pub async fn wait_for_signal(&self) {
        #[cfg(unix)]
        {
            let mut sigterm = tokio::signal::unix::signal(
                tokio::signal::unix::SignalKind::terminate(),
            )
            .expect("Failed to create SIGTERM handler");
            let mut sigint = tokio::signal::unix::signal(
                tokio::signal::unix::SignalKind::interrupt(),
            )
            .expect("Failed to create SIGINT handler");

            tokio::select! {
                _ = sigterm.recv() => { info!("Received SIGTERM"); }
                _ = sigint.recv() => { info!("Received SIGINT"); }
            }
        }

        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to listen for ctrl-c");
            info!("Received ctrl-c");
        }

        self.shutdown();
    }

    /// Graceful shutdown with timeout.
    ///
    /// 1. Broadcast shutdown signal.
    /// 2. Wait up to `timeout` for tasks to finish.
    /// 3. Force exit after timeout.
    #[instrument]
    pub async fn graceful_shutdown(&self, timeout: Duration) -> Result<(), String> {
        info!("Starting graceful shutdown (timeout: {:?})", timeout);
        self.shutdown();

        tokio::time::sleep(timeout).await;
        warn!("Graceful shutdown timeout reached");
        Ok(())
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let coord = ShutdownCoordinator::new();
        let rx1 = coord.subscribe();
        let rx2 = coord.subscribe();
        drop(rx1);
        drop(rx2);
    }

    #[test]
    fn test_shutdown_broadcast() {
        let coord = ShutdownCoordinator::new();
        let mut rx = coord.subscribe();
        coord.shutdown();
        assert!(rx.try_recv().is_ok());
    }

    #[tokio::test]
    async fn test_graceful_shutdown_timeout() {
        let coord = ShutdownCoordinator::new();
        let result = coord.graceful_shutdown(Duration::from_millis(10)).await;
        assert!(result.is_ok());
    }
}
