//! Tools: read_world_entries and write_world_entry — manage world/lore book entries in SQLite.
//!
//! `ReadWorldEntriesTool` retrieves entries by ID or search query (LIKE-based).
//! `WriteWorldEntryTool` creates or updates entries, snapshots before update,
//! and sends `UndoAvailable` events to the frontend.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::dispatcher::{validate_against_schema, Tool};
use crate::agent::events::WsEvent;
use crate::db::Database;
use crate::versioning::VersionStore;

// ---------------------------------------------------------------------------
// ReadWorldEntriesTool
// ---------------------------------------------------------------------------

/// Tool that retrieves world/lore book entries from the local SQLite database.
///
/// Supports lookup by exact ID or text search (LIKE) against label, content,
/// and keywords fields.
pub struct ReadWorldEntriesTool {
    db: Arc<Mutex<Database>>,
}

impl ReadWorldEntriesTool {
    /// Create a new `ReadWorldEntriesTool`.
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl Tool for ReadWorldEntriesTool {
    fn name(&self) -> &str {
        "read_world_entries"
    }

    fn description(&self) -> &str {
        "Retrieve world/lore book entries by ID or search query"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Specific entry ID to retrieve"
                },
                "query": {
                    "type": "string",
                    "description": "Search text to match against label, content, or keywords"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default 10)",
                    "default": 10
                }
            }
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)?;

        // At least one of `id` or `query` must be provided
        let has_id = args.get("id").and_then(|v| v.as_str()).is_some();
        let has_query = args.get("query").and_then(|v| v.as_str()).is_some();
        if !has_id && !has_query {
            anyhow::bail!("At least one of 'id' or 'query' must be provided");
        }

        Ok(())
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;

        let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(10) as usize;

        if let Some(id) = args.get("id").and_then(|v| v.as_str()) {
            // Lookup by exact ID
            debug!(id = %id, "Reading world entry by ID");

            let result: Option<Value> = db
                .conn()
                .query_row(
                    "SELECT id, label, content, keywords, metadata, created_at, updated_at \
                     FROM world_entries WHERE id = ?1",
                    rusqlite::params![id],
                    |row| {
                        Ok(serde_json::json!({
                            "id": row.get::<_, String>(0)?,
                            "label": row.get::<_, String>(1)?,
                            "content": row.get::<_, String>(2)?,
                            "keywords": row.get::<_, Option<String>>(3)?,
                            "metadata": row.get::<_, Option<String>>(4)?,
                            "created_at": row.get::<_, String>(5)?,
                            "updated_at": row.get::<_, String>(6)?,
                        }))
                    },
                )
                .optional()?;

            match result {
                Some(entry) => Ok(serde_json::json!([entry])),
                None => Ok(serde_json::json!([]))
            }
        } else if let Some(query) = args.get("query").and_then(|v| v.as_str()) {
            // Search by LIKE against label, content, keywords
            debug!(query = %query, limit = limit, "Searching world entries");

            let pattern = format!("%{}%", query);
            let mut stmt = db.conn().prepare(
                "SELECT id, label, content, keywords, metadata, created_at, updated_at \
                 FROM world_entries \
                 WHERE label LIKE ?1 OR content LIKE ?1 OR keywords LIKE ?1 \
                 LIMIT ?2",
            )?;

            let entries: Vec<Value> = stmt
                .query_map(rusqlite::params![pattern, limit as i64], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "label": row.get::<_, String>(1)?,
                        "content": row.get::<_, String>(2)?,
                        "keywords": row.get::<_, Option<String>>(3)?,
                        "metadata": row.get::<_, Option<String>>(4)?,
                        "created_at": row.get::<_, String>(5)?,
                        "updated_at": row.get::<_, String>(6)?,
                    }))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(serde_json::json!(entries))
        } else {
            // Should not reach here due to validate_args, but handle gracefully
            anyhow::bail!("At least one of 'id' or 'query' must be provided")
        }
    }
}

// ---------------------------------------------------------------------------
// WriteWorldEntryTool
// ---------------------------------------------------------------------------

/// Tool that creates or updates a world/lore book entry in SQLite.
///
/// On update, snapshots the previous state via `VersionStore` for undo support.
/// Sends an `UndoAvailable` event after both create and update operations.
pub struct WriteWorldEntryTool {
    db: Arc<Mutex<Database>>,
    version_store: Arc<VersionStore>,
    event_tx: tokio::sync::mpsc::Sender<WsEvent>,
}

impl WriteWorldEntryTool {
    /// Create a new `WriteWorldEntryTool`.
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
impl Tool for WriteWorldEntryTool {
    fn name(&self) -> &str {
        "write_world_entry"
    }

    fn description(&self) -> &str {
        "Create or update a world/lore book entry"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "If provided, update existing entry; if omitted, create new"
                },
                "label": {
                    "type": "string",
                    "description": "Entry title/label (required for create)"
                },
                "content": {
                    "type": "string",
                    "description": "Entry content (required for create)"
                },
                "keywords": {
                    "type": "string",
                    "description": "Comma-separated keywords for lorebook matching"
                },
                "metadata": {
                    "type": "object",
                    "description": "Arbitrary JSON metadata"
                },
                "project_id": {
                    "type": "string",
                    "description": "Associate with a project"
                }
            }
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)?;

        let is_update = args.get("id").and_then(|v| v.as_str()).is_some();

        if !is_update {
            // For create, label and content are required
            if args.get("label").and_then(|v| v.as_str()).is_none() {
                anyhow::bail!("'label' is required when creating a new world entry");
            }
            if args.get("content").and_then(|v| v.as_str()).is_none() {
                anyhow::bail!("'content' is required when creating a new world entry");
            }
        }

        Ok(())
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        if let Some(id) = args.get("id").and_then(|v| v.as_str()) {
            self.update_entry(id, &args).await
        } else {
            self.create_entry(&args).await
        }
    }
}

impl WriteWorldEntryTool {
    /// Update an existing world entry by ID.
    async fn update_entry(&self, id: &str, args: &Value) -> Result<Value> {
        // Collect field values from args (all String types, Send-safe)
        let label_val = args.get("label").and_then(|v| v.as_str()).map(|s| s.to_string());
        let content_val = args.get("content").and_then(|v| v.as_str()).map(|s| s.to_string());
        let keywords_val = args.get("keywords").and_then(|v| v.as_str()).map(|s| s.to_string());
        let metadata_val = args.get("metadata").map(|v| serde_json::to_string(v).unwrap_or_default());
        let project_id_val = args.get("project_id").and_then(|v| v.as_str()).map(|s| s.to_string());

        // Perform all DB operations synchronously within the mutex lock
        let (updated_fields, updated_entry) = {
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;

            // 1. Read current entry from SQLite
            let current_data: Value = db
                .conn()
                .query_row(
                    "SELECT id, project_id, label, content, keywords, metadata, created_at, updated_at \
                     FROM world_entries WHERE id = ?1",
                    rusqlite::params![id],
                    |row| {
                        Ok(serde_json::json!({
                            "id": row.get::<_, String>(0)?,
                            "project_id": row.get::<_, Option<String>>(1)?,
                            "label": row.get::<_, String>(2)?,
                            "content": row.get::<_, String>(3)?,
                            "keywords": row.get::<_, Option<String>>(4)?,
                            "metadata": row.get::<_, Option<String>>(5)?,
                            "created_at": row.get::<_, String>(6)?,
                            "updated_at": row.get::<_, String>(7)?,
                        }))
                    },
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("World entry not found: {}", id))?;

            // 2. Snapshot current state via version_store (uses its own lock)
            drop(db);
            self.version_store.snapshot(
                "world_entry",
                id,
                &current_data,
                "Before write_world_entry update",
            )?;

            // Re-acquire lock for the update
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;

            // 3. Build SET clause with only provided fields
            let mut set_clauses: Vec<&str> = Vec::new();
            let mut updated_fields: Vec<String> = Vec::new();

            if label_val.is_some() {
                set_clauses.push("label = ?");
                updated_fields.push("label".to_string());
            }
            if content_val.is_some() {
                set_clauses.push("content = ?");
                updated_fields.push("content".to_string());
            }
            if keywords_val.is_some() {
                set_clauses.push("keywords = ?");
                updated_fields.push("keywords".to_string());
            }
            if metadata_val.is_some() {
                set_clauses.push("metadata = ?");
                updated_fields.push("metadata".to_string());
            }
            if project_id_val.is_some() {
                set_clauses.push("project_id = ?");
                updated_fields.push("project_id".to_string());
            }

            if set_clauses.is_empty() {
                return Ok(serde_json::json!({
                    "success": true,
                    "message": "No fields to update were provided",
                    "id": id
                }));
            }

            // Always update updated_at
            set_clauses.push("updated_at = CURRENT_TIMESTAMP");

            // 4. Execute UPDATE with dynamic params
            let sql = format!(
                "UPDATE world_entries SET {} WHERE id = ?",
                set_clauses.join(", ")
            );

            // Build params vector in the same order as set_clauses
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            if let Some(ref v) = label_val { params.push(Box::new(v.clone())); }
            if let Some(ref v) = content_val { params.push(Box::new(v.clone())); }
            if let Some(ref v) = keywords_val { params.push(Box::new(v.clone())); }
            if let Some(ref v) = metadata_val { params.push(Box::new(v.clone())); }
            if let Some(ref v) = project_id_val { params.push(Box::new(v.clone())); }
            params.push(Box::new(id.to_string()));

            let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            db.conn().execute(&sql, param_refs.as_slice())?;

            debug!(id = %id, fields = ?updated_fields, "World entry updated");

            // TODO: Index updated entry in tantivy search index (task 6.2)

            // 5. Read back the updated entry
            let entry = db.conn().query_row(
                "SELECT id, project_id, label, content, keywords, metadata, created_at, updated_at \
                 FROM world_entries WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "project_id": row.get::<_, Option<String>>(1)?,
                        "label": row.get::<_, String>(2)?,
                        "content": row.get::<_, String>(3)?,
                        "keywords": row.get::<_, Option<String>>(4)?,
                        "metadata": row.get::<_, Option<String>>(5)?,
                        "created_at": row.get::<_, String>(6)?,
                        "updated_at": row.get::<_, String>(7)?,
                    }))
                },
            )?;

            (updated_fields, entry)
        };

        // 6. Send UndoAvailable event (after releasing the mutex)
        let summary = format!("World entry updated: {}", updated_fields.join(", "));
        let _ = self.event_tx.send(WsEvent::UndoAvailable {
            entity_type: "world_entry".to_string(),
            entity_id: id.to_string(),
            summary: summary.clone(),
        }).await;

        Ok(serde_json::json!({
            "success": true,
            "id": id,
            "updated_fields": updated_fields,
            "message": summary,
            "entry": updated_entry
        }))
    }

    /// Create a new world entry.
    async fn create_entry(&self, args: &Value) -> Result<Value> {
        let id = uuid::Uuid::new_v4().to_string();
        let label = args["label"].as_str().unwrap(); // validated in validate_args
        let content = args["content"].as_str().unwrap(); // validated in validate_args
        let keywords = args.get("keywords").and_then(|v| v.as_str());
        let metadata = args.get("metadata").map(|v| serde_json::to_string(v).unwrap_or_default());
        let project_id = args.get("project_id").and_then(|v| v.as_str());

        {
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
            db.conn().execute(
                "INSERT INTO world_entries (id, project_id, label, content, keywords, metadata) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    id,
                    project_id,
                    label,
                    content,
                    keywords,
                    metadata,
                ],
            )?;
        }

        debug!(id = %id, label = %label, "World entry created");

        // TODO: Index new entry in tantivy search index (task 6.2)

        // Send UndoAvailable event (so user can undo the creation)
        let summary = format!("World entry created: {}", label);
        let _ = self.event_tx.send(WsEvent::UndoAvailable {
            entity_type: "world_entry".to_string(),
            entity_id: id.clone(),
            summary: summary.clone(),
        }).await;

        // Read back the created entry
        let entry = {
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
            db.conn().query_row(
                "SELECT id, project_id, label, content, keywords, metadata, created_at, updated_at \
                 FROM world_entries WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "project_id": row.get::<_, Option<String>>(1)?,
                        "label": row.get::<_, String>(2)?,
                        "content": row.get::<_, String>(3)?,
                        "keywords": row.get::<_, Option<String>>(4)?,
                        "metadata": row.get::<_, Option<String>>(5)?,
                        "created_at": row.get::<_, String>(6)?,
                        "updated_at": row.get::<_, String>(7)?,
                    }))
                },
            )?
        };

        Ok(serde_json::json!({
            "success": true,
            "id": id,
            "message": summary,
            "entry": entry
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
    fn test_read_tool_schema_validation() {
        let db = setup_db();
        let tool = ReadWorldEntriesTool::new(db);

        // Valid: has id
        let valid_id = serde_json::json!({"id": "entry-1"});
        assert!(tool.validate_args(&valid_id).is_ok());

        // Valid: has query
        let valid_query = serde_json::json!({"query": "dragon"});
        assert!(tool.validate_args(&valid_query).is_ok());

        // Valid: has both
        let valid_both = serde_json::json!({"id": "entry-1", "query": "dragon"});
        assert!(tool.validate_args(&valid_both).is_ok());

        // Invalid: neither id nor query
        let invalid = serde_json::json!({"limit": 5});
        assert!(tool.validate_args(&invalid).is_err());

        // Invalid: empty object
        let empty = serde_json::json!({});
        assert!(tool.validate_args(&empty).is_err());
    }

    #[test]
    fn test_write_tool_schema_validation_create() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        let tool = WriteWorldEntryTool::new(db, vs, tx);

        // Valid create: has label and content
        let valid = serde_json::json!({"label": "Dragon Lore", "content": "Dragons are..."});
        assert!(tool.validate_args(&valid).is_ok());

        // Invalid create: missing content
        let no_content = serde_json::json!({"label": "Dragon Lore"});
        assert!(tool.validate_args(&no_content).is_err());

        // Invalid create: missing label
        let no_label = serde_json::json!({"content": "Dragons are..."});
        assert!(tool.validate_args(&no_label).is_err());
    }

    #[test]
    fn test_write_tool_schema_validation_update() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        let tool = WriteWorldEntryTool::new(db, vs, tx);

        // Valid update: has id (label/content optional)
        let valid = serde_json::json!({"id": "entry-1", "content": "Updated content"});
        assert!(tool.validate_args(&valid).is_ok());

        // Valid update: id only (no fields to update, but valid)
        let id_only = serde_json::json!({"id": "entry-1"});
        assert!(tool.validate_args(&id_only).is_ok());
    }

    #[tokio::test]
    async fn test_read_by_id_not_found() {
        let db = setup_db();
        let tool = ReadWorldEntriesTool::new(db);

        let args = serde_json::json!({"id": "nonexistent"});
        let result = tool.execute(args).await.unwrap();
        assert_eq!(result, serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_create_and_read_entry() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(16);

        let write_tool = WriteWorldEntryTool::new(db.clone(), vs, tx);
        let read_tool = ReadWorldEntriesTool::new(db.clone());

        // Create an entry
        let create_args = serde_json::json!({
            "label": "Dragon Lore",
            "content": "Dragons are ancient creatures of immense power.",
            "keywords": "dragon,lore,creature"
        });
        let create_result = write_tool.execute(create_args).await.unwrap();
        assert_eq!(create_result["success"], true);
        let entry_id = create_result["id"].as_str().unwrap().to_string();

        // Read by ID
        let read_args = serde_json::json!({"id": entry_id});
        let read_result = read_tool.execute(read_args).await.unwrap();
        let entries = read_result.as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["label"], "Dragon Lore");
        assert_eq!(entries[0]["content"], "Dragons are ancient creatures of immense power.");
        assert_eq!(entries[0]["keywords"], "dragon,lore,creature");
    }

    #[tokio::test]
    async fn test_search_entries() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(16);

        let write_tool = WriteWorldEntryTool::new(db.clone(), vs, tx);
        let read_tool = ReadWorldEntriesTool::new(db.clone());

        // Create two entries
        write_tool.execute(serde_json::json!({
            "label": "Dragon Lore",
            "content": "Dragons are ancient creatures.",
            "keywords": "dragon,lore"
        })).await.unwrap();

        write_tool.execute(serde_json::json!({
            "label": "Elf History",
            "content": "Elves are immortal beings.",
            "keywords": "elf,history"
        })).await.unwrap();

        // Search for "dragon"
        let search_result = read_tool.execute(serde_json::json!({"query": "dragon"})).await.unwrap();
        let entries = search_result.as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["label"], "Dragon Lore");

        // Search for "ancient" (in content)
        let search_result = read_tool.execute(serde_json::json!({"query": "ancient"})).await.unwrap();
        let entries = search_result.as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["label"], "Dragon Lore");

        // Search with limit
        let search_result = read_tool.execute(serde_json::json!({"query": "e", "limit": 1})).await.unwrap();
        let entries = search_result.as_array().unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[tokio::test]
    async fn test_update_entry() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);

        let write_tool = WriteWorldEntryTool::new(db.clone(), vs.clone(), tx);
        let read_tool = ReadWorldEntriesTool::new(db.clone());

        // Create an entry
        let create_result = write_tool.execute(serde_json::json!({
            "label": "Dragon Lore",
            "content": "Original content.",
            "keywords": "dragon"
        })).await.unwrap();
        let entry_id = create_result["id"].as_str().unwrap().to_string();

        // Drain the create UndoAvailable event
        let _ = rx.recv().await;

        // Update the entry
        let update_result = write_tool.execute(serde_json::json!({
            "id": entry_id,
            "content": "Updated content about dragons.",
            "keywords": "dragon,fire,lore"
        })).await.unwrap();
        assert_eq!(update_result["success"], true);
        assert!(update_result["updated_fields"].as_array().unwrap().contains(&serde_json::json!("content")));
        assert!(update_result["updated_fields"].as_array().unwrap().contains(&serde_json::json!("keywords")));

        // Verify UndoAvailable was sent for update
        let event = rx.recv().await.unwrap();
        match event {
            WsEvent::UndoAvailable { entity_type, entity_id, .. } => {
                assert_eq!(entity_type, "world_entry");
                assert_eq!(entity_id, entry_id);
            }
            _ => panic!("Expected UndoAvailable event"),
        }

        // Read back and verify
        let read_result = read_tool.execute(serde_json::json!({"id": entry_id})).await.unwrap();
        let entries = read_result.as_array().unwrap();
        assert_eq!(entries[0]["content"], "Updated content about dragons.");
        assert_eq!(entries[0]["keywords"], "dragon,fire,lore");
        // Label should be unchanged
        assert_eq!(entries[0]["label"], "Dragon Lore");
    }

    #[tokio::test]
    async fn test_update_nonexistent_entry() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(16);

        let write_tool = WriteWorldEntryTool::new(db, vs, tx);

        let result = write_tool.execute(serde_json::json!({
            "id": "nonexistent",
            "content": "New content"
        })).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_update_snapshots_for_undo() {
        let db = setup_db();
        let vs = setup_version_store(db.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(16);

        let write_tool = WriteWorldEntryTool::new(db.clone(), vs.clone(), tx);

        // Create an entry
        let create_result = write_tool.execute(serde_json::json!({
            "label": "Test Entry",
            "content": "Original content."
        })).await.unwrap();
        let entry_id = create_result["id"].as_str().unwrap().to_string();

        // Update the entry
        write_tool.execute(serde_json::json!({
            "id": entry_id,
            "content": "Updated content."
        })).await.unwrap();

        // Verify a snapshot was created
        let versions = vs.list_versions("world_entry", &entry_id).unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].data["content"], "Original content.");
    }
}
