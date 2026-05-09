use std::sync::Arc;

use anyhow::Result;
use tracing::info;

mod agent;
mod config;
mod context;
mod db;
mod http;
mod llm;
mod lorebook;
mod prompts;
mod search;
mod tools;
mod versioning;
mod ws;

#[tokio::main]
async fn main() -> Result<()> {
    // Handle --version flag before anything else
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("eni-sidecar {}", env!("ENI_VERSION"));
        return Ok(());
    }

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
    info!(ws_port = config.listen_port, http_port = config.http_port, "Configuration loaded");

    // Initialize SQLite database
    let database = db::Database::open(&config.db_path)?;
    let db = Arc::new(std::sync::Mutex::new(database));

    // Initialize search index
    let search_index_path = std::path::Path::new(&config.db_path)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("search_index");
    let search_index = Arc::new(search::SearchIndex::new(&search_index_path)?);

    // Initialize version store
    let version_store = Arc::new(versioning::VersionStore::new(Arc::clone(&db)));

    // Initialize LLM client with default model profile
    let default_profile = config
        .default_model()
        .cloned()
        .unwrap_or_else(|| config::ModelProfile {
            name: "default".to_string(),
            base_url: "http://localhost:11434/v1".to_string(),
            api_key: "none".to_string(),
            model: "llama3".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            is_default: true,
        });
    let llm_client = llm::LlmClient::new(default_profile);

    // Initialize tool dispatcher
    let tool_dispatcher = Arc::new(tools::ToolDispatcher::new());

    // Build shared application state for WebSocket server
    let http_port = config.http_port;
    let ws_state = Arc::new(ws::AppState {
        config,
        db: Arc::clone(&db),
        version_store,
        llm_client,
        tool_dispatcher,
        search_index: Arc::clone(&search_index),
    });

    // Build shared state for HTTP API
    let http_state = Arc::new(http::HttpState {
        db: Arc::clone(&db),
        search_index,
    });

    info!("ENI Sidecar is ready. Starting servers...");

    // Start the HTTP API server in a background task
    let http_handle = tokio::spawn(async move {
        if let Err(e) = http::start(http_state, http_port).await {
            tracing::error!(error = %e, "HTTP API server failed");
        }
    });

    // Start the WebSocket server (runs forever)
    let ws_handle = tokio::spawn(async move {
        if let Err(e) = ws::start(ws_state).await {
            tracing::error!(error = %e, "WebSocket server failed");
        }
    });

    // Wait for either server to finish (they shouldn't under normal operation)
    tokio::select! {
        _ = http_handle => {
            tracing::error!("HTTP server exited unexpectedly");
        }
        _ = ws_handle => {
            tracing::error!("WebSocket server exited unexpectedly");
        }
    }

    Ok(())
}
