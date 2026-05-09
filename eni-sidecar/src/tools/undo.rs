//! Tools: undo_change and list_versions — version history management.
//!
//! `UndoChangeTool` pops the most recent snapshot from entity_versions and
//! restores the entity to that state (writes back via ST API for characters,
//! updates SQLite for world entries).
//!
//! `ListVersionsTool` returns the version history for an entity.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex as TokioMutex;
use tracing::debug;

use super::dispatcher::{validate_against_schema, Tool};
use super::st_client::{CharacterData, StClient};
use crate::db::Database;
use crate::versioning::VersionStore;

// ---------------------------------------------------------------------------
// UndoChangeTool
// ---------------------------------------------------------------------------

/// Tool that reverts the most recent change to a specified entity.
///
/// For characters: restores via ST REST API.
/// For world entries: restores directly in SQLite.
pub struct UndoChangeTool {
    version_store: Arc<VersionStore>,
    st_client: Arc<TokioMutex<StClient>>,
    db: Arc<Mutex<Database>>,
}

impl UndoChangeTool {
    /// Create a new `UndoChangeTool`.
    pub fn new(
        version_store: Arc<VersionStore>,
        st_client: Arc<TokioMutex<StClient>>,
        db: Arc<Mutex<Database>>,
    ) -> Self {
        Self {
            version_store,
            st_client,
            db,
        }
    }
}

#[async_trait]
impl Tool for UndoChangeTool {
    fn name(&self) -> &str {
        "undo_change"
    }

    fn description(&self) -> &str {
        "Revert the most recent change to a character or world entry, restoring the previous version"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "entity_type": {
                    "type": "string",
                    "description": "The type of entity to undo: 'character' or 'world_entry'",
                    "enum": ["character", "world_entry"]
                },
                "entity_id": {
                    "type": "string",
                    "description": "The entity identifier (character name or world entry ID)"
                }
            },
            "required": ["entity_type", "entity_id"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let entity_type = args["entity_type"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: entity_type"))?;

        let entity_id = args["entity_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: entity_id"))?;

        debug!(entity_type = %entity_type, entity_id = %entity_id, "Undoing change");

        // Pop the most recent version
        let previous_data = self.version_store.undo(entity_type, entity_id)?;

        let Some(data) = previous_data else {
            return Ok(serde_json::json!({
                "success": false,
                "message": format!("No version history found for {} '{}'", entity_type, entity_id)
            }));
        };

        // Restore the entity based on type
        match entity_type {
            "character" => {
                // Deserialize the snapshot back into CharacterData and write via ST API
                let character: CharacterData = serde_json::from_value(data.clone())
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize character snapshot: {}", e))?;

                // Resolve character name to avatar_url for the merge-attributes endpoint
                let avatar_url = {
                    let mut client = self.st_client.lock().await;
                    let characters = client.get_characters().await?;
                    characters
                        .iter()
                        .find(|c| c.name.eq_ignore_ascii_case(&character.name))
                        .map(|c| c.avatar.clone())
                        .unwrap_or_else(|| format!("{}.png", character.name))
                };

                let update_value = serde_json::to_value(&character)?;
                let mut client = self.st_client.lock().await;
                client.edit_character(&avatar_url, &update_value).await?;

                debug!(name = %entity_id, "Character restored from version history");

                Ok(serde_json::json!({
                    "success": true,
                    "entity_type": "character",
                    "entity_id": entity_id,
                    "message": format!("Character '{}' reverted to previous version", entity_id),
                    "restored_data": data
                }))
            }
            "world_entry" => {
                // Restore world entry directly in SQLite
                let label = data["label"].as_str().unwrap_or("");
                let content = data["content"].as_str().unwrap_or("");
                let keywords = data["keywords"].as_str();
                let metadata = data.get("metadata").and_then(|v| v.as_str());

                let db = self.db.lock()
                    .map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;

                let rows = db.conn().execute(
                    "UPDATE world_entries SET label = ?1, content = ?2, keywords = ?3, metadata = ?4, updated_at = CURRENT_TIMESTAMP WHERE id = ?5",
                    rusqlite::params![label, content, keywords, metadata, entity_id],
                )?;

                if rows == 0 {
                    // Entry was deleted — re-create it
                    let project_id = data["project_id"].as_str();
                    db.conn().execute(
                        "INSERT INTO world_entries (id, project_id, label, content, keywords, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        rusqlite::params![entity_id, project_id, label, content, keywords, metadata],
                    )?;
                }

                debug!(id = %entity_id, "World entry restored from version history");

                Ok(serde_json::json!({
                    "success": true,
                    "entity_type": "world_entry",
                    "entity_id": entity_id,
                    "message": format!("World entry '{}' reverted to previous version", entity_id),
                    "restored_data": data
                }))
            }
            _ => {
                anyhow::bail!("Unsupported entity type for undo: '{}'", entity_type)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ListVersionsTool
// ---------------------------------------------------------------------------

/// Tool that returns the version history for a specified entity.
pub struct ListVersionsTool {
    version_store: Arc<VersionStore>,
}

impl ListVersionsTool {
    /// Create a new `ListVersionsTool`.
    pub fn new(version_store: Arc<VersionStore>) -> Self {
        Self { version_store }
    }
}

#[async_trait]
impl Tool for ListVersionsTool {
    fn name(&self) -> &str {
        "list_versions"
    }

    fn description(&self) -> &str {
        "List the version history for a character or world entry, showing timestamps and change summaries"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "entity_type": {
                    "type": "string",
                    "description": "The type of entity: 'character' or 'world_entry'",
                    "enum": ["character", "world_entry"]
                },
                "entity_id": {
                    "type": "string",
                    "description": "The entity identifier (character name or world entry ID)"
                }
            },
            "required": ["entity_type", "entity_id"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let entity_type = args["entity_type"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: entity_type"))?;

        let entity_id = args["entity_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: entity_id"))?;

        debug!(entity_type = %entity_type, entity_id = %entity_id, "Listing versions");

        let versions = self.version_store.list_versions(entity_type, entity_id)?;

        let version_list: Vec<Value> = versions
            .iter()
            .map(|v| {
                serde_json::json!({
                    "id": v.id,
                    "summary": v.summary,
                    "created_at": v.created_at,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "entity_type": entity_type,
            "entity_id": entity_id,
            "versions": version_list,
            "total": version_list.len()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_version_store() -> (Arc<VersionStore>, Arc<Mutex<Database>>) {
        let db = Database::open(":memory:").unwrap();
        let db = Arc::new(Mutex::new(db));
        let vs = Arc::new(VersionStore::new(db.clone()));
        (vs, db)
    }

    // --- ListVersionsTool tests ---

    #[test]
    fn test_list_versions_schema_validation() {
        let (vs, _db) = setup_version_store();
        let tool = ListVersionsTool::new(vs);

        // Valid
        let valid = serde_json::json!({
            "entity_type": "character",
            "entity_id": "kael"
        });
        assert!(tool.validate_args(&valid).is_ok());

        // Invalid: missing entity_type
        let no_type = serde_json::json!({"entity_id": "kael"});
        assert!(tool.validate_args(&no_type).is_err());

        // Invalid: missing entity_id
        let no_id = serde_json::json!({"entity_type": "character"});
        assert!(tool.validate_args(&no_id).is_err());

        // Invalid: bad entity_type enum
        let bad_type = serde_json::json!({
            "entity_type": "invalid",
            "entity_id": "kael"
        });
        assert!(tool.validate_args(&bad_type).is_err());
    }

    #[tokio::test]
    async fn test_list_versions_empty() {
        let (vs, _db) = setup_version_store();
        let tool = ListVersionsTool::new(vs);

        let result = tool.execute(serde_json::json!({
            "entity_type": "character",
            "entity_id": "nonexistent"
        })).await.unwrap();

        assert_eq!(result["total"], 0);
        assert_eq!(result["versions"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_list_versions_with_history() {
        let (vs, _db) = setup_version_store();

        // Create some snapshots
        vs.snapshot("character", "kael", &serde_json::json!({"name": "Kael", "v": 1}), "Version 1").unwrap();
        vs.snapshot("character", "kael", &serde_json::json!({"name": "Kael", "v": 2}), "Version 2").unwrap();

        let tool = ListVersionsTool::new(vs);

        let result = tool.execute(serde_json::json!({
            "entity_type": "character",
            "entity_id": "kael"
        })).await.unwrap();

        assert_eq!(result["total"], 2);
        let versions = result["versions"].as_array().unwrap();
        assert_eq!(versions[0]["summary"], "Version 2"); // newest first
        assert_eq!(versions[1]["summary"], "Version 1");
    }

    // --- UndoChangeTool tests ---

    #[test]
    fn test_undo_schema_validation() {
        let (_vs, _db) = setup_version_store();
        let (_tx, _rx) = tokio::sync::mpsc::channel::<crate::agent::events::WsEvent>(16);
        // We can't easily create a real StClient for unit tests, so we test schema only
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "entity_type": {
                    "type": "string",
                    "enum": ["character", "world_entry"]
                },
                "entity_id": {
                    "type": "string"
                }
            },
            "required": ["entity_type", "entity_id"]
        });

        // Valid
        let valid = serde_json::json!({
            "entity_type": "character",
            "entity_id": "kael"
        });
        assert!(validate_against_schema(&schema, &valid).is_ok());

        // Invalid: bad enum
        let bad = serde_json::json!({
            "entity_type": "invalid",
            "entity_id": "kael"
        });
        assert!(validate_against_schema(&schema, &bad).is_err());
    }

    #[tokio::test]
    async fn test_undo_world_entry() {
        let (vs, db) = setup_version_store();

        // Create a world entry in the database
        {
            let db_lock = db.lock().unwrap();
            db_lock.conn().execute(
                "INSERT INTO world_entries (id, label, content, keywords) VALUES ('entry-1', 'Dragon Lore', 'Updated content', 'dragon')",
                [],
            ).unwrap();
        }

        // Snapshot the original state
        vs.snapshot(
            "world_entry",
            "entry-1",
            &serde_json::json!({
                "label": "Dragon Lore",
                "content": "Original content",
                "keywords": "dragon"
            }),
            "Before update",
        ).unwrap();

        // Create a mock StClient (won't be used for world_entry undo)
        // We need to test the world_entry path which doesn't use StClient
        // For this test, we'll create the tool with a dummy client
        let _st_config = crate::config::StConfig {
            base_url: "http://localhost:9999".to_string(),
            api_key: None,
        };
        // We can't create a real StClient without network, so we test the DB path directly
        // by calling version_store.undo and then verifying the DB state

        let undone = vs.undo("world_entry", "entry-1").unwrap();
        assert!(undone.is_some());
        let data = undone.unwrap();
        assert_eq!(data["content"], "Original content");

        // Simulate what UndoChangeTool would do for world_entry
        let label = data["label"].as_str().unwrap();
        let content = data["content"].as_str().unwrap();
        let keywords = data["keywords"].as_str();

        {
            let db_lock = db.lock().unwrap();
            db_lock.conn().execute(
                "UPDATE world_entries SET label = ?1, content = ?2, keywords = ?3, updated_at = CURRENT_TIMESTAMP WHERE id = ?4",
                rusqlite::params![label, content, keywords, "entry-1"],
            ).unwrap();
        }

        // Verify the entry was restored
        let db_lock = db.lock().unwrap();
        let restored_content: String = db_lock.conn().query_row(
            "SELECT content FROM world_entries WHERE id = 'entry-1'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(restored_content, "Original content");
    }
}
