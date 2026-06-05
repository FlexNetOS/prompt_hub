#![forbid(unsafe_code)]
// WIP server: some handlers/specs are scaffolded ahead of being wired into routes.
#![allow(dead_code)]

use anyhow::Result;
use axum::serve;
use clap::Parser;
use prompt_hub::config::HubConfig;
use tokio::net::TcpListener;
use tracing::info;

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

    // Create router with state
    let app = server::create_router(state);

    // Serve with graceful shutdown
    serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shutdown complete");
    Ok(())
}

/// Wait for SIGTERM or SIGINT to trigger graceful shutdown.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .expect("SIGINT handler");

        tokio::select! {
            _ = sigterm.recv() => info!("Received SIGTERM, shutting down gracefully"),
            _ = sigint.recv() => info!("Received SIGINT, shutting down gracefully"),
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.expect("ctrl-c handler");
        info!("Received ctrl-c, shutting down gracefully");
    }
}
