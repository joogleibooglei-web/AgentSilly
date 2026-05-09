//! HTTP request handlers for the axum REST API.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use uuid::Uuid;

use super::HttpState;

// ─── Health ──────────────────────────────────────────────────────────────────

/// Sidecar version (from Cargo.toml).
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

/// GET /health — returns 200 with sidecar version and status.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: VERSION,
    })
}

// ─── Config ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ConfigResponse {
    pub model_profiles: Vec<ModelProfileInfo>,
    pub active_profile: Option<ActiveProfileInfo>,
    pub post_card_prompt: Option<String>,
    pub st_base_url: Option<String>,
}

#[derive(Serialize)]
pub struct ActiveProfileInfo {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

#[derive(Serialize)]
pub struct ModelProfileInfo {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: i64,
    pub is_default: bool,
}

/// GET /config — return current config (model profiles, post-card prompt, ST URL).
pub async fn get_config(
    State(state): State<Arc<HttpState>>,
) -> Result<Json<ConfigResponse>, (StatusCode, String)> {
    let db = state.db.lock().map_err(|e| {
        error!(error = %e, "Database lock poisoned");
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    // Fetch model profiles
    let mut stmt = db
        .conn()
        .prepare("SELECT id, name, base_url, model, temperature, max_tokens, is_default FROM model_profiles")
        .map_err(|e| {
            error!(error = %e, "Failed to prepare model_profiles query");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
        })?;

    let profiles: Vec<ModelProfileInfo> = stmt
        .query_map([], |row| {
            Ok(ModelProfileInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                base_url: row.get(2)?,
                model: row.get(3)?,
                temperature: row.get(4)?,
                max_tokens: row.get(5)?,
                is_default: row.get::<_, i32>(6)? != 0,
            })
        })
        .map_err(|e| {
            error!(error = %e, "Failed to query model_profiles");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Fetch post-card prompt from config table
    let post_card_prompt: Option<String> = db
        .conn()
        .query_row(
            "SELECT value FROM config WHERE key = 'post_card_prompt'",
            [],
            |row| row.get(0),
        )
        .ok()
        .and_then(|v: String| serde_json::from_str::<String>(&v).ok().or(Some(v)));

    // Fetch ST base URL from config table
    let st_base_url: Option<String> = db
        .conn()
        .query_row(
            "SELECT value FROM config WHERE key = 'st_base_url'",
            [],
            |row| row.get(0),
        )
        .ok()
        .and_then(|v: String| serde_json::from_str::<String>(&v).ok().or(Some(v)));

    // Fetch user-configured model profile from config table
    let get_config_value = |key: &str| -> Option<String> {
        db.conn()
            .query_row(
                "SELECT value FROM config WHERE key = ?1",
                rusqlite::params![key],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| serde_json::from_str::<String>(&v).ok().or(Some(v)))
    };

    let active_base_url = get_config_value("model_profile.baseUrl");
    let active_api_key = get_config_value("model_profile.apiKey");
    let active_model = get_config_value("model_profile.model");
    let active_temperature = get_config_value("model_profile.temperature")
        .and_then(|v| v.parse::<f64>().ok());
    let active_max_tokens = get_config_value("model_profile.maxTokens")
        .and_then(|v| v.parse::<u32>().ok());

    let active_profile = if active_base_url.is_some() || active_api_key.is_some() || active_model.is_some() {
        Some(ActiveProfileInfo {
            base_url: active_base_url,
            api_key: active_api_key,
            model: active_model,
            temperature: active_temperature,
            max_tokens: active_max_tokens,
        })
    } else {
        None
    };

    Ok(Json(ConfigResponse {
        model_profiles: profiles,
        active_profile,
        post_card_prompt,
        st_base_url,
    }))
}

#[derive(Deserialize)]
pub struct ConfigUpdate {
    pub key: String,
    pub value: String,
}

/// PUT /config — update config values, persist to SQLite.
pub async fn put_config(
    State(state): State<Arc<HttpState>>,
    Json(payload): Json<ConfigUpdate>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Validate allowed config keys
    let allowed_keys = [
        "post_card_prompt",
        "st_base_url",
        "st_api_key",
        "max_iterations",
    ];

    if !allowed_keys.contains(&payload.key.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid config key: '{}'. Allowed keys: {:?}", payload.key, allowed_keys),
        ));
    }

    let db = state.db.lock().map_err(|e| {
        error!(error = %e, "Database lock poisoned");
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    db.conn()
        .execute(
            "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
            rusqlite::params![&payload.key, &payload.value],
        )
        .map_err(|e| {
            error!(error = %e, key = %payload.key, "Failed to update config");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to update config".to_string())
        })?;

    info!(key = %payload.key, "Config updated via HTTP API");
    Ok(StatusCode::NO_CONTENT)
}

// ─── Conversations ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: Option<String>,
    pub created_at: String,
}

/// GET /conversations — list conversations (id, title, created_at).
pub async fn list_conversations(
    State(state): State<Arc<HttpState>>,
) -> Result<Json<Vec<ConversationSummary>>, (StatusCode, String)> {
    let db = state.db.lock().map_err(|e| {
        error!(error = %e, "Database lock poisoned");
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    let mut stmt = db
        .conn()
        .prepare(
            "SELECT id, title, created_at FROM conversations WHERE archived = 0 ORDER BY created_at DESC",
        )
        .map_err(|e| {
            error!(error = %e, "Failed to prepare conversations query");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
        })?;

    let conversations: Vec<ConversationSummary> = stmt
        .query_map([], |row| {
            Ok(ConversationSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get::<_, String>(2).unwrap_or_default(),
            })
        })
        .map_err(|e| {
            error!(error = %e, "Failed to query conversations");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(conversations))
}

#[derive(Serialize)]
pub struct MessageInfo {
    pub id: String,
    pub role: String,
    pub content: String,
    pub metadata: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct ConversationDetail {
    pub id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub messages: Vec<MessageInfo>,
}

/// GET /conversations/:id — return messages for a conversation.
pub async fn get_conversation(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> Result<Json<ConversationDetail>, (StatusCode, String)> {
    let db = state.db.lock().map_err(|e| {
        error!(error = %e, "Database lock poisoned");
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    // Fetch conversation metadata
    let (title, created_at): (Option<String>, String) = db
        .conn()
        .query_row(
            "SELECT title, created_at FROM conversations WHERE id = ?1",
            rusqlite::params![&id],
            |row| Ok((row.get(0)?, row.get::<_, String>(1).unwrap_or_default())),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                (StatusCode::NOT_FOUND, format!("Conversation '{}' not found", id))
            }
            _ => {
                error!(error = %e, "Failed to query conversation");
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
            }
        })?;

    // Fetch messages
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT id, role, content, metadata, created_at FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
        )
        .map_err(|e| {
            error!(error = %e, "Failed to prepare messages query");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
        })?;

    let messages: Vec<MessageInfo> = stmt
        .query_map(rusqlite::params![&id], |row| {
            Ok(MessageInfo {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                metadata: row.get(3)?,
                created_at: row.get::<_, String>(4).unwrap_or_default(),
            })
        })
        .map_err(|e| {
            error!(error = %e, "Failed to query messages");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(ConversationDetail {
        id,
        title,
        created_at,
        messages,
    }))
}

// ─── Reference Documents ─────────────────────────────────────────────────────

/// Maximum number of reference documents per project.
const MAX_DOCUMENTS_PER_PROJECT: usize = 20;

/// Maximum combined text size (5MB).
const MAX_COMBINED_SIZE_BYTES: usize = 5 * 1024 * 1024;

/// Target chunk size in characters for document chunking.
const CHUNK_SIZE: usize = 1000;

/// Overlap between chunks in characters.
const CHUNK_OVERLAP: usize = 200;

#[derive(Deserialize)]
pub struct UploadDocumentRequest {
    /// Filename for the document.
    pub filename: String,
    /// Full text content of the document.
    pub content: String,
    /// Optional project ID to associate with.
    pub project_id: Option<String>,
}

#[derive(Serialize)]
pub struct DocumentInfo {
    pub id: String,
    pub filename: String,
    pub size_bytes: i64,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct UploadDocumentResponse {
    pub id: String,
    pub filename: String,
    pub size_bytes: usize,
    pub chunks_created: usize,
}

/// POST /documents — upload text/markdown document, chunk it, store in SQLite, index in tantivy.
pub async fn upload_document(
    State(state): State<Arc<HttpState>>,
    Json(payload): Json<UploadDocumentRequest>,
) -> Result<(StatusCode, Json<UploadDocumentResponse>), (StatusCode, String)> {
    let content_size = payload.content.len();

    if payload.filename.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Filename is required".to_string()));
    }

    if payload.content.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Content is required".to_string()));
    }

    let db = state.db.lock().map_err(|e| {
        error!(error = %e, "Database lock poisoned");
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    // Enforce document count limit
    let project_filter = payload.project_id.as_deref().unwrap_or("__global__");
    let doc_count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM reference_documents WHERE COALESCE(project_id, '__global__') = ?1",
            rusqlite::params![project_filter],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if doc_count as usize >= MAX_DOCUMENTS_PER_PROJECT {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "Maximum of {} documents per project reached. Remove a document first.",
                MAX_DOCUMENTS_PER_PROJECT
            ),
        ));
    }

    // Enforce combined size limit
    let current_size: i64 = db
        .conn()
        .query_row(
            "SELECT COALESCE(SUM(size_bytes), 0) FROM reference_documents WHERE COALESCE(project_id, '__global__') = ?1",
            rusqlite::params![project_filter],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if (current_size as usize) + content_size > MAX_COMBINED_SIZE_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Combined document size would exceed {}MB limit. Current: {}KB, new: {}KB.",
                MAX_COMBINED_SIZE_BYTES / (1024 * 1024),
                current_size / 1024,
                content_size / 1024,
            ),
        ));
    }

    // Generate document ID
    let doc_id = Uuid::new_v4().to_string();

    // Insert the document
    db.conn()
        .execute(
            "INSERT INTO reference_documents (id, project_id, filename, content, size_bytes) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                &doc_id,
                &payload.project_id,
                &payload.filename,
                &payload.content,
                content_size as i64,
            ],
        )
        .map_err(|e| {
            error!(error = %e, "Failed to insert reference document");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to store document".to_string())
        })?;

    // Chunk the document
    let chunks = chunk_text(&payload.content, CHUNK_SIZE, CHUNK_OVERLAP);
    let chunk_count = chunks.len();

    for (idx, chunk_content) in chunks.iter().enumerate() {
        let chunk_id = Uuid::new_v4().to_string();

        db.conn()
            .execute(
                "INSERT INTO document_chunks (id, document_id, content, chunk_index) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![&chunk_id, &doc_id, chunk_content, idx as i64],
            )
            .map_err(|e| {
                error!(error = %e, chunk_index = idx, "Failed to insert document chunk");
                (StatusCode::INTERNAL_SERVER_ERROR, "Failed to store document chunk".to_string())
            })?;

        // Index the chunk in tantivy
        if let Err(e) = state.search_index.index_document(
            &chunk_id,
            "document_chunk",
            &format!("{} [chunk {}]", payload.filename, idx),
            chunk_content,
        ) {
            warn!(error = %e, chunk_id = %chunk_id, "Failed to index document chunk (non-fatal)");
        }
    }

    info!(
        doc_id = %doc_id,
        filename = %payload.filename,
        size = content_size,
        chunks = chunk_count,
        "Reference document uploaded and indexed"
    );

    Ok((
        StatusCode::CREATED,
        Json(UploadDocumentResponse {
            id: doc_id,
            filename: payload.filename,
            size_bytes: content_size,
            chunks_created: chunk_count,
        }),
    ))
}

/// GET /documents — list uploaded documents (id, filename, size).
pub async fn list_documents(
    State(state): State<Arc<HttpState>>,
) -> Result<Json<Vec<DocumentInfo>>, (StatusCode, String)> {
    let db = state.db.lock().map_err(|e| {
        error!(error = %e, "Database lock poisoned");
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    let mut stmt = db
        .conn()
        .prepare("SELECT id, filename, size_bytes, created_at FROM reference_documents ORDER BY created_at DESC")
        .map_err(|e| {
            error!(error = %e, "Failed to prepare documents query");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
        })?;

    let documents: Vec<DocumentInfo> = stmt
        .query_map([], |row| {
            Ok(DocumentInfo {
                id: row.get(0)?,
                filename: row.get(1)?,
                size_bytes: row.get(2)?,
                created_at: row.get::<_, String>(3).unwrap_or_default(),
            })
        })
        .map_err(|e| {
            error!(error = %e, "Failed to query documents");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(documents))
}

/// DELETE /documents/:id — remove document and its chunks from DB and index.
pub async fn delete_document(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let db = state.db.lock().map_err(|e| {
        error!(error = %e, "Database lock poisoned");
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    // Check the document exists
    let exists: bool = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM reference_documents WHERE id = ?1",
            rusqlite::params![&id],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false);

    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Document '{}' not found", id),
        ));
    }

    // Get chunk IDs for index removal
    let chunk_ids: Vec<String> = {
        let mut stmt = db
            .conn()
            .prepare("SELECT id FROM document_chunks WHERE document_id = ?1")
            .map_err(|e| {
                error!(error = %e, "Failed to prepare chunk query");
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
            })?;

        let ids: Vec<String> = stmt
            .query_map(rusqlite::params![&id], |row| row.get(0))
            .map_err(|e| {
                error!(error = %e, "Failed to query chunks");
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
            })?
            .filter_map(|r| r.ok())
            .collect();
        ids
    };

    // Remove chunks from search index
    for chunk_id in &chunk_ids {
        if let Err(e) = state.search_index.remove_document(chunk_id) {
            warn!(error = %e, chunk_id = %chunk_id, "Failed to remove chunk from index (non-fatal)");
        }
    }

    // Delete chunks from DB (cascade should handle this, but be explicit)
    db.conn()
        .execute(
            "DELETE FROM document_chunks WHERE document_id = ?1",
            rusqlite::params![&id],
        )
        .map_err(|e| {
            error!(error = %e, "Failed to delete document chunks");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete document".to_string())
        })?;

    // Delete the document itself
    db.conn()
        .execute(
            "DELETE FROM reference_documents WHERE id = ?1",
            rusqlite::params![&id],
        )
        .map_err(|e| {
            error!(error = %e, "Failed to delete reference document");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete document".to_string())
        })?;

    info!(doc_id = %id, chunks_removed = chunk_ids.len(), "Reference document deleted");
    Ok(StatusCode::NO_CONTENT)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Split text into overlapping chunks for indexing.
///
/// Uses a simple character-based chunking strategy with overlap to preserve
/// context at chunk boundaries.
fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    if text.len() <= chunk_size {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;

    while start < chars.len() {
        let end = (start + chunk_size).min(chars.len());
        let chunk: String = chars[start..end].iter().collect();
        chunks.push(chunk);

        if end >= chars.len() {
            break;
        }

        // Move forward by (chunk_size - overlap) to create overlap
        start += chunk_size.saturating_sub(overlap);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_empty() {
        let chunks = chunk_text("", 100, 20);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_text_small() {
        let chunks = chunk_text("Hello world", 100, 20);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello world");
    }

    #[test]
    fn test_chunk_text_exact_size() {
        let text = "a".repeat(100);
        let chunks = chunk_text(&text, 100, 20);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunk_text_with_overlap() {
        // 250 chars, chunk_size=100, overlap=20 → step=80
        // Chunks: [0..100], [80..180], [160..250]
        let text = "a".repeat(250);
        let chunks = chunk_text(&text, 100, 20);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 100);
        assert_eq!(chunks[1].len(), 100);
        assert_eq!(chunks[2].len(), 90); // remaining
    }

    #[test]
    fn test_chunk_text_no_overlap() {
        let text = "a".repeat(300);
        let chunks = chunk_text(&text, 100, 0);
        assert_eq!(chunks.len(), 3);
        for chunk in &chunks {
            assert_eq!(chunk.len(), 100);
        }
    }
}
