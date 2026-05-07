use anyhow::Result;
use tracing::info;

mod agent;
mod config;
mod context;
mod db;
mod http;
mod llm;
mod search;
mod tools;
mod versioning;
mod ws;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing subscriber for structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("ENI Sidecar starting up...");

    // Load configuration from TOML file
    let config = config::load_config()?;
    info!(port = config.listen_port, "Configuration loaded");

    // Initialize SQLite database
    let _db = db::Database::open(&config.db_path)?;

    // Start the server
    info!(
        "Starting ENI sidecar server on port {}",
        config.listen_port
    );

    // Placeholder: server startup will be implemented in later tasks
    info!("ENI Sidecar is ready.");

    Ok(())
}
