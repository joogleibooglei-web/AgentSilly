//! HTTP API module — axum router with REST endpoints.
//!
//! Provides health checks, configuration management, conversation history,
//! and reference document upload/management endpoints.

mod handlers;

use std::sync::Arc;

use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::db::Database;
use crate::search::SearchIndex;

/// Shared state for HTTP handlers.
pub struct HttpState {
    pub db: Arc<std::sync::Mutex<Database>>,
    pub search_index: Arc<SearchIndex>,
}

/// Build the axum router with all HTTP API routes.
pub fn build_router(state: Arc<HttpState>) -> Router {
    Router::new()
        // Health
        .route("/health", get(handlers::health))
        // Config
        .route("/config", get(handlers::get_config))
        .route("/config", put(handlers::put_config))
        // Conversations
        .route("/conversations", get(handlers::list_conversations))
        .route("/conversations/{id}", get(handlers::get_conversation))
        // Reference documents
        .route("/documents", post(handlers::upload_document))
        .route("/documents", get(handlers::list_documents))
        .route("/documents/{id}", delete(handlers::delete_document))
        .with_state(state)
}

/// Start the HTTP API server on the given port.
pub async fn start(state: Arc<HttpState>, port: u16) -> anyhow::Result<()> {
    let app = build_router(state);
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(address = %addr, "HTTP API server listening");
    axum::serve(listener, app).await?;
    Ok(())
}
