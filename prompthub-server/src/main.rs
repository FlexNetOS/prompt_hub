#![forbid(unsafe_code)]
// WIP server: some handlers/specs are scaffolded ahead of being wired into routes.
#![allow(dead_code)]

use std::net::SocketAddr;

use anyhow::Result;
use axum::serve;
use clap::Parser;
use prompt_hub::config::HubConfig;
use tokio::net::TcpListener;
use tracing::{info, warn};

mod middleware;
mod openapi;
mod responses;
mod routes;
mod server;
mod state;

use state::AppState;

#[derive(Parser, Debug)]
#[command(name = "prompthub-server")]
#[command(about = "HTTP API server for prompt-hub")]
#[command(version)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Database file path (optional, defaults to in-memory)
    #[arg(long)]
    db_path: Option<String>,

    /// Host address to bind to
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing with JSON formatting in release, pretty in debug
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::new(&args.log_level)
                .add_directive("tower_http=info".parse()?),
        )
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true);

    #[cfg(debug_assertions)]
    subscriber.pretty().init();
    #[cfg(not(debug_assertions))]
    subscriber.json().init();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "Starting prompthub-server"
    );

    // Load configuration
    let config = HubConfig::load().unwrap_or_default();
    let db_path = args
        .db_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var("PROMPTHUB_DB_PATH")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("prompthub.db"))
        });

    info!(db_path = %db_path.display(), "Using database");

    // Create app state with real PromptHub
    let state = AppState::new(&db_path, config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize PromptHub: {e}"))?;

    let addr = format!("{}:{}", args.host, args.port);
    let listener = TcpListener::bind(&addr).await?;

    info!(addr = %addr, "Server listening");

    // Keep a handle to the hub (and its shutdown coordinator) so we can run an
    // orderly hub shutdown once the server stops serving. `create_router`
    // consumes the AppState, so we clone the Arc first.
    let hub = state.hub.clone();
    let coordinator = hub.shutdown_coordinator();

    // Create router with state
    let app = server::create_router(state);

    // Serve until the hub's shutdown coordinator fires on SIGTERM/SIGINT. The
    // coordinator is the single shutdown rendezvous shared with the hub's
    // background daemons.
    // GovernorLayer's default PeerIpKeyExtractor needs ConnectInfo<SocketAddr>
    // in request extensions; without `into_make_service_with_connect_info` every
    // request fails the rate-limiter key extraction ("Unable to extract key!" → 500).
    let serve_coordinator = coordinator.clone();
    serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        let mut rx = serve_coordinator.subscribe();
        // Listen for OS signals; this also broadcasts to every other
        // subscriber (the hub's daemons). On handler-install failure we log
        // and fall back to awaiting the broadcast so the server can still
        // be shut down programmatically.
        tokio::select! {
            res = serve_coordinator.wait_for_signal() => {
                if let Err(e) = res {
                    warn!("signal handler failed, awaiting broadcast instead: {e}");
                    let _ = rx.recv().await;
                }
            }
            _ = rx.recv() => {}
        }
        info!("Server received shutdown signal, draining connections");
    })
    .await?;

    // Server has stopped accepting connections; run the orderly hub shutdown
    // (stop daemons, flush WAL to disk).
    if let Err(e) = hub.shutdown().await {
        warn!("hub shutdown reported an error: {e}");
    }

    info!("Server shutdown complete");
    Ok(())
}
