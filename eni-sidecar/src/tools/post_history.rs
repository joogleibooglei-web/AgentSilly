//! Tools: read_post_history and write_post_history — manage post-history instructions in SQLite.
//!
//! `ReadPostHistoryTool` retrieves the current post-history instructions from the config table.
//! `WritePostHistoryTool` updates the post-history instructions, snapshots before writing,
//! and sends an `UndoAvailable` event to the frontend.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::dispatcher::{validate_against_schema, Tool};
use crate::agent::events::WsEvent;
use crate::db::Database;
use crate::versioning::VersionStore;

/// The config key used to store post-history instructions.
const POST_HISTORY_KEY: &str = "post_history_instructions";

// ---------------------------------------------------------------------------
// ReadPostHistoryTool
// ---------------------------------------------------------------------------

/// Tool that retrieves the current post-history instructions from the config table.
pub struct ReadPostHistoryTool {
    db: Arc<Mutex<Database>>,
}

impl ReadPostHistoryTool {
    /// Create a new `ReadPostHistoryTool`.
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl Tool for ReadPostHistoryTool {
    fn name(&self) -> &str {
        "read_post_history"
    }

    fn description(&self) -> &str {
        "Retrieve the current post-history instructions"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, _args: Value) -> Result<Value> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;

        let result: Option<String> = db
            .conn()
            .query_row(
                "SELECT value FROM config WHERE key = ?1",
                rusqlite::params![POST_HISTORY_KEY],
                |row| row.get(0),
            )
            .optional()?;

        let content = result.unwrap_or_default();

        debug!(content_len = content.len(), "Read post-history instructions");

        Ok(serde_json::json!({ "content": content }))
    }
}

// ---------------------------------------------------------------------------
// WritePostHistoryTool
// ---------------------------------------------------------------------------

/// Tool that updates the post-history instructions in the config table.
///
/// Snapshots the previous state via `VersionStore` before writing, and sends
/// an `UndoAvailable` event to the frontend after a successful write.
pub struct WritePostHistoryTool {
    db: Arc<Mutex<Database>>,
    version_store: Arc<VersionStore>,
    event_tx: tokio::sync::mpsc::Sender<WsEvent>,
}

impl WritePostHistoryTool {
    /// Create a new `WritePostHistoryTool`.
    pub fn new(
        db: Arc<Mutex<Database>>,
        version_store: Arc<VersionStore>,
        event_tx: tokio::sync::mpsc::Sender<WsEvent>,
    ) -> Self {
        Self {
            db,
            version_store,
            event_tx,
        }
    }
}

#[async_trait]
impl Tool for WritePostHistoryTool {
    fn name(&self) -> &str {
        "write_post_history"
    }

    fn description(&self) -> &str {
        "Update the post-history instructions"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The new post-history instructions text"
                }
            },
            "required": ["content"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: content"))?;

        // 1. Read current value from config table (may be empty/missing)
        let current_value = {
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
            db.conn()
                .query_row(
                    "SELECT value FROM config WHERE key = ?1",
                    rusqlite::params![POST_HISTORY_KEY],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .unwrap_or_default()
        };

        // 2. Snapshot the current value for undo support
        let snapshot_data = serde_json::json!({ "content": current_value });
        self.version_store.snapshot(
            "post_history",
            "instructions",
            &snapshot_data,
            "Before write_post_history",
        )?;

        // 3. Upsert the new value
        {
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
            db.conn().execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
                rusqlite::params![POST_HISTORY_KEY, content],
            )?;
        }

        debug!(content_len = content.len(), "Post-history instructions updated");

        // 4. Send UndoAvailable event to frontend
        let _ = self.event_tx.send(WsEvent::UndoAvailable {
            entity_type: "post_history".to_string(),
            entity_id: "instructions".to_string(),
            summary: "Post-history instructions updated".to_string(),
        }).await;

        // 5. Return success
        Ok(serde_json::json!({
            "success": true,
            "message": "Post-history instructions updated"
        }))
    }
}

/// Extension trait to add `.optional()` to rusqlite results (local to this module).
trait OptionalExt<T> {
    fn optional(self) -> std::result::Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for std::result::Result<T, rusqlite::Error> {
    fn optional(self) -> std::result::Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Arc<Mutex<Database>> {
        let db = Database::open(":memory:").unwrap();
        Arc::new(Mutex::new(db))
    }

    fn setup_version_store(db: Arc<Mutex<Database>>) -> Arc<VersionStore> {
        Arc::new(VersionStore::new(db))
    }

    #[test]
    fn test_read_tool_name_and_description() {
        let db = setup_db();
        let tool = ReadPostHistoryTool::new(db);
        assert_eq!(tool.name(), "read_post_history");
        assert_eq!(tool.description(), "Retrieve the current post-history instructions");
    }

    #[test]
    fn test_write_tool_name_and_description() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        let tool = WritePostHistoryTool::new(db, vs, tx);
        assert_eq!(tool.name(), "write_post_history");
        assert_eq!(tool.description(), "Update the post-history instructions");
    }

    #[test]
    fn test_read_tool_schema_validation() {
        let db = setup_db();
        let tool = ReadPostHistoryTool::new(db);

        // Empty object is valid (no parameters required)
        let valid = serde_json::json!({});
        assert!(tool.validate_args(&valid).is_ok());
    }

    #[test]
    fn test_write_tool_schema_validation() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        let tool = WritePostHistoryTool::new(db, vs, tx);

        // Valid: has content
        let valid = serde_json::json!({"content": "Some instructions"});
        assert!(tool.validate_args(&valid).is_ok());

        // Invalid: missing content
        let invalid = serde_json::json!({});
        assert!(tool.validate_args(&invalid).is_err());

        // Invalid: content is not a string
        let wrong_type = serde_json::json!({"content": 123});
        assert!(tool.validate_args(&wrong_type).is_err());
    }

    #[tokio::test]
    async fn test_read_empty_returns_empty_content() {
        let db = setup_db();
        let tool = ReadPostHistoryTool::new(db);

        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert_eq!(result["content"], "");
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(16);

        let write_tool = WritePostHistoryTool::new(db.clone(), vs, tx);
        let read_tool = ReadPostHistoryTool::new(db.clone());

        // Write instructions
        let write_result = write_tool.execute(serde_json::json!({
            "content": "Always end with a question."
        })).await.unwrap();
        assert_eq!(write_result["success"], true);
        assert_eq!(write_result["message"], "Post-history instructions updated");

        // Read back
        let read_result = read_tool.execute(serde_json::json!({})).await.unwrap();
        assert_eq!(read_result["content"], "Always end with a question.");
    }

    #[tokio::test]
    async fn test_write_overwrites_previous() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(16);

        let write_tool = WritePostHistoryTool::new(db.clone(), vs, tx);
        let read_tool = ReadPostHistoryTool::new(db.clone());

        // Write first value
        write_tool.execute(serde_json::json!({
            "content": "First instructions"
        })).await.unwrap();

        // Write second value
        write_tool.execute(serde_json::json!({
            "content": "Second instructions"
        })).await.unwrap();

        // Read should return the latest
        let read_result = read_tool.execute(serde_json::json!({})).await.unwrap();
        assert_eq!(read_result["content"], "Second instructions");
    }

    #[tokio::test]
    async fn test_write_snapshots_for_undo() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(16);

        let write_tool = WritePostHistoryTool::new(db.clone(), vs.clone(), tx);

        // Write first value (snapshots empty state)
        write_tool.execute(serde_json::json!({
            "content": "First instructions"
        })).await.unwrap();

        // Verify snapshot was created with empty content
        let versions = vs.list_versions("post_history", "instructions").unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].data["content"], "");

        // Write second value (snapshots first value)
        write_tool.execute(serde_json::json!({
            "content": "Second instructions"
        })).await.unwrap();

        let versions = vs.list_versions("post_history", "instructions").unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].data["content"], "First instructions");
    }

    #[tokio::test]
    async fn test_write_sends_undo_available_event() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);

        let write_tool = WritePostHistoryTool::new(db.clone(), vs, tx);

        write_tool.execute(serde_json::json!({
            "content": "New instructions"
        })).await.unwrap();

        // Verify UndoAvailable event was sent
        let event = rx.recv().await.unwrap();
        match event {
            WsEvent::UndoAvailable { entity_type, entity_id, summary } => {
                assert_eq!(entity_type, "post_history");
                assert_eq!(entity_id, "instructions");
                assert_eq!(summary, "Post-history instructions updated");
            }
            _ => panic!("Expected UndoAvailable event"),
        }
    }
}
