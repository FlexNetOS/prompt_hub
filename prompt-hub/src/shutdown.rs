#![forbid(unsafe_code)]

//! Graceful shutdown coordination.
//!
//! The [`ShutdownCoordinator`] is the single rendezvous point used to bring the
//! hub (and the axum server, and any spawned background daemons) to an orderly
//! stop. Every long-lived task subscribes to the broadcast channel and unwinds
//! when it fires; [`PromptHub::shutdown`](crate::PromptHub::shutdown) drives the
//! coordinator and then flushes storage so the process can exit without data
//! loss.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{info, instrument, warn};

use crate::error::{HubError, Result};

/// Graceful shutdown coordinator.
///
/// Uses a [`tokio::sync::broadcast`] channel so every spawned task and the axum
/// server can subscribe to a single shutdown signal, plus an [`AtomicBool`] so
/// the shutdown state is observable and broadcasting is idempotent (calling
/// [`shutdown`](Self::shutdown) twice fires the signal once).
///
/// Cloning a coordinator yields another handle to the *same* underlying channel
/// and state (the inner fields are `Arc`-shared), so all clones agree on whether
/// shutdown has begun.
#[derive(Debug, Clone)]
pub struct ShutdownCoordinator {
    tx: broadcast::Sender<()>,
    /// `true` once a shutdown has been initiated. Shared across clones.
    initiated: Arc<AtomicBool>,
}

impl ShutdownCoordinator {
    /// Create a new coordinator with no shutdown in progress.
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(1);
        Self {
            tx,
            initiated: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Subscribe to the shutdown signal.
    ///
    /// The returned receiver resolves once when [`shutdown`](Self::shutdown) (or
    /// [`wait_for_signal`](Self::wait_for_signal)) fires.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }

    /// Whether a shutdown has already been initiated on this coordinator.
    pub fn is_shutting_down(&self) -> bool {
        self.initiated.load(Ordering::SeqCst)
    }

    /// Number of live subscribers currently awaiting the shutdown signal.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }

    /// Initiate graceful shutdown by broadcasting the signal.
    ///
    /// Idempotent: the signal is broadcast only on the first call; subsequent
    /// calls are no-ops. Returns `true` if this call performed the broadcast,
    /// `false` if shutdown was already in progress.
    #[instrument(skip(self))]
    pub fn shutdown(&self) -> bool {
        // `swap` returns the previous value; if it was already `true`, another
        // caller has already broadcast and we must not fire again.
        if self.initiated.swap(true, Ordering::SeqCst) {
            return false;
        }
        info!("Graceful shutdown signal broadcast");
        // A send error only means there are no live subscribers, which is fine.
        let _ = self.tx.send(());
        true
    }

    /// Wait for `SIGTERM` or `SIGINT`, then broadcast shutdown.
    ///
    /// On non-Unix platforms falls back to [`tokio::signal::ctrl_c`].
    ///
    /// # Errors
    /// Returns [`HubError::Internal`] if the OS signal handlers cannot be
    /// installed (registering the signal handler is fallible and must not panic
    /// the process — `#![forbid(unsafe_code)]` aside, a crashed signal task is
    /// itself a shutdown defect).
    #[instrument(skip(self))]
    pub async fn wait_for_signal(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};

            let mut sigterm = signal(SignalKind::terminate()).map_err(|e| {
                HubError::Internal(format!("failed to install SIGTERM handler: {e}"))
            })?;
            let mut sigint = signal(SignalKind::interrupt()).map_err(|e| {
                HubError::Internal(format!("failed to install SIGINT handler: {e}"))
            })?;

            tokio::select! {
                _ = sigterm.recv() => { info!("Received SIGTERM"); }
                _ = sigint.recv() => { info!("Received SIGINT"); }
            }
        }

        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .map_err(|e| HubError::Internal(format!("failed to listen for ctrl-c: {e}")))?;
            info!("Received ctrl-c");
        }

        self.shutdown();
        Ok(())
    }

    /// Broadcast shutdown and wait up to `timeout` for subscribers to unwind.
    ///
    /// 1. Broadcast the shutdown signal (idempotent).
    /// 2. Poll until every subscriber has dropped its receiver, or until
    ///    `timeout` elapses.
    ///
    /// Returns `Ok(())` whether or not all subscribers drained in time; a
    /// timeout is logged as a warning rather than treated as a hard error, since
    /// the caller will proceed to flush storage and exit regardless.
    #[instrument(skip(self))]
    pub async fn graceful_shutdown(&self, timeout: Duration) -> Result<()> {
        info!("Starting graceful shutdown (timeout: {:?})", timeout);
        self.shutdown();

        let deadline = tokio::time::Instant::now() + timeout;
        let poll = Duration::from_millis(10);
        while self.subscriber_count() > 0 {
            if tokio::time::Instant::now() >= deadline {
                warn!(
                    remaining = self.subscriber_count(),
                    "Graceful shutdown timeout reached with subscribers still live"
                );
                return Ok(());
            }
            tokio::time::sleep(poll.min(timeout)).await;
        }

        info!("All subscribers drained; graceful shutdown complete");
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
    fn test_new_is_not_shutting_down() {
        let coord = ShutdownCoordinator::new();
        assert!(!coord.is_shutting_down());
        let rx1 = coord.subscribe();
        let rx2 = coord.subscribe();
        assert_eq!(coord.subscriber_count(), 2);
        drop(rx1);
        drop(rx2);
    }

    #[test]
    fn test_shutdown_broadcast_reaches_subscriber() {
        let coord = ShutdownCoordinator::new();
        let mut rx = coord.subscribe();
        assert!(coord.shutdown());
        assert!(coord.is_shutting_down());
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn test_shutdown_is_idempotent() {
        let coord = ShutdownCoordinator::new();
        // First call fires, second is a no-op.
        assert!(coord.shutdown());
        assert!(!coord.shutdown());
        assert!(coord.is_shutting_down());
    }

    #[test]
    fn test_clones_share_state() {
        let coord = ShutdownCoordinator::new();
        let clone = coord.clone();
        let mut rx = clone.subscribe();
        // Shutdown on the original is visible to the clone and its subscribers.
        assert!(coord.shutdown());
        assert!(clone.is_shutting_down());
        assert!(rx.try_recv().is_ok());
        // The clone cannot re-fire the (shared) signal.
        assert!(!clone.shutdown());
    }

    #[tokio::test]
    async fn test_signaling_completes_a_waiting_subscriber() {
        let coord = ShutdownCoordinator::new();
        let mut rx = coord.subscribe();
        let waiter = tokio::spawn(async move { rx.recv().await });

        // Give the waiter a moment to park on recv(), then signal.
        tokio::task::yield_now().await;
        assert!(coord.shutdown());

        let res = waiter.await.expect("waiter task panicked");
        assert!(res.is_ok(), "subscriber should observe the shutdown signal");
    }

    #[tokio::test]
    async fn test_graceful_shutdown_returns_when_subscribers_drain() {
        let coord = ShutdownCoordinator::new();
        let mut rx = coord.subscribe();
        let drainer = tokio::spawn(async move {
            // Observe the signal, then drop the receiver to "drain".
            let _ = rx.recv().await;
        });

        coord
            .graceful_shutdown(Duration::from_secs(5))
            .await
            .expect("graceful_shutdown should succeed");
        drainer.await.expect("drainer task panicked");
        assert_eq!(coord.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn test_graceful_shutdown_tolerates_timeout() {
        let coord = ShutdownCoordinator::new();
        // Hold a subscriber that never drains.
        let _held = coord.subscribe();
        let result = coord.graceful_shutdown(Duration::from_millis(20)).await;
        assert!(result.is_ok());
        assert!(coord.is_shutting_down());
    }
}
