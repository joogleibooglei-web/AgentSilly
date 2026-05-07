//! Database module — SQLite connection and schema management.
//!
//! Uses rusqlite for synchronous SQLite access. The database stores conversations,
//! messages, projects, tasks, world entries, reference documents, entity versions,
//! configuration, and model profiles.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;
use tracing::info;

/// Database handle wrapping a rusqlite connection.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) the SQLite database at the given path and run migrations.
    pub fn open(path: &str) -> Result<Self> {
        let conn = if path == ":memory:" {
            Connection::open_in_memory()
                .context("Failed to open in-memory SQLite database")?
        } else {
            // Ensure parent directory exists
            if let Some(parent) = Path::new(path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("Failed to create database directory: {}", parent.display()))?;
                }
            }
            Connection::open(path)
                .with_context(|| format!("Failed to open SQLite database at: {}", path))?
        };

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        // Enable foreign keys
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;

        let db = Self { conn };
        db.run_migrations()?;

        info!(path = %path, "Database opened and migrations applied");
        Ok(db)
    }

    /// Run all schema migrations (create tables if they don't exist).
    fn run_migrations(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA_SQL)
            .context("Failed to apply database schema migrations")?;
        Ok(())
    }

    /// Get a reference to the underlying connection.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Get a mutable reference to the underlying connection.
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

/// Full database schema matching the design document.
const SCHEMA_SQL: &str = r#"
-- Conversations
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    title TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    archived INTEGER DEFAULT 0
);

-- Messages within conversations
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL,  -- 'user', 'assistant', 'tool_call', 'tool_result', 'system'
    content TEXT NOT NULL,
    metadata TEXT,  -- JSON: tool call details, token counts, etc.
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation
    ON messages(conversation_id, created_at);

-- Projects
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    metadata TEXT,  -- JSON: genre, setting, tone
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Tasks within projects
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    status TEXT DEFAULT 'planned',  -- planned, in_progress, complete
    description TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_tasks_project
    ON tasks(project_id);

-- World / Lore book entries
CREATE TABLE IF NOT EXISTS world_entries (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    label TEXT NOT NULL,
    content TEXT NOT NULL,
    keywords TEXT,  -- comma-separated for lorebook matching
    metadata TEXT,  -- JSON
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_world_entries_project
    ON world_entries(project_id);

-- Reference documents (uploaded by user for context injection)
CREATE TABLE IF NOT EXISTS reference_documents (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    filename TEXT NOT NULL,
    content TEXT NOT NULL,
    size_bytes INTEGER,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_reference_documents_project
    ON reference_documents(project_id);

-- Chunks of reference documents (for search indexing)
CREATE TABLE IF NOT EXISTS document_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES reference_documents(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    chunk_index INTEGER,
    embedding BLOB  -- optional: vector embedding
);

CREATE INDEX IF NOT EXISTS idx_document_chunks_document
    ON document_chunks(document_id);

-- Version history for undo support
CREATE TABLE IF NOT EXISTS entity_versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type TEXT NOT NULL,  -- 'character', 'world_entry'
    entity_id TEXT NOT NULL,
    data TEXT NOT NULL,  -- JSON snapshot of entity state
    summary TEXT,  -- human-readable change description
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_entity_versions_entity
    ON entity_versions(entity_type, entity_id, created_at DESC);

-- Key-value configuration store
CREATE TABLE IF NOT EXISTS config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Model profiles (persisted from config, can be updated at runtime)
CREATE TABLE IF NOT EXISTS model_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    api_key TEXT NOT NULL,
    model TEXT NOT NULL,
    temperature REAL DEFAULT 0.7,
    max_tokens INTEGER DEFAULT 4096,
    is_default INTEGER DEFAULT 0
);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let db = Database::open(":memory:").unwrap();
        // Verify tables exist by querying sqlite_master
        let count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        // We expect 9 tables: conversations, messages, projects, tasks,
        // world_entries, reference_documents, document_chunks, entity_versions,
        // config, model_profiles
        assert_eq!(count, 10);
    }

    #[test]
    fn test_migrations_idempotent() {
        let db = Database::open(":memory:").unwrap();
        // Running migrations again should not fail
        db.run_migrations().unwrap();
        db.run_migrations().unwrap();
    }

    #[test]
    fn test_insert_conversation_and_message() {
        let db = Database::open(":memory:").unwrap();

        db.conn()
            .execute(
                "INSERT INTO conversations (id, title) VALUES (?1, ?2)",
                rusqlite::params!["conv-1", "Test Conversation"],
            )
            .unwrap();

        db.conn()
            .execute(
                "INSERT INTO messages (id, conversation_id, role, content) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params!["msg-1", "conv-1", "user", "Hello ENI"],
            )
            .unwrap();

        let content: String = db
            .conn()
            .query_row(
                "SELECT content FROM messages WHERE id = ?1",
                rusqlite::params!["msg-1"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(content, "Hello ENI");
    }

    #[test]
    fn test_foreign_key_enforcement() {
        let db = Database::open(":memory:").unwrap();

        // Inserting a message with a non-existent conversation_id should fail
        let result = db.conn().execute(
            "INSERT INTO messages (id, conversation_id, role, content) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["msg-1", "nonexistent", "user", "Hello"],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_entity_versions_table() {
        let db = Database::open(":memory:").unwrap();

        db.conn()
            .execute(
                "INSERT INTO entity_versions (entity_type, entity_id, data, summary) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params!["character", "char-1", r#"{"name":"Kael"}"#, "Initial creation"],
            )
            .unwrap();

        let data: String = db
            .conn()
            .query_row(
                "SELECT data FROM entity_versions WHERE entity_type = ?1 AND entity_id = ?2",
                rusqlite::params!["character", "char-1"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(data, r#"{"name":"Kael"}"#);
    }

    #[test]
    fn test_model_profiles_table() {
        let db = Database::open(":memory:").unwrap();

        db.conn()
            .execute(
                "INSERT INTO model_profiles (id, name, base_url, api_key, model, temperature, max_tokens, is_default) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params!["prof-1", "fast", "https://api.openai.com/v1", "sk-test", "gpt-4o-mini", 0.5, 2048, 1],
            )
            .unwrap();

        let is_default: i32 = db
            .conn()
            .query_row(
                "SELECT is_default FROM model_profiles WHERE id = ?1",
                rusqlite::params!["prof-1"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(is_default, 1);
    }
}
