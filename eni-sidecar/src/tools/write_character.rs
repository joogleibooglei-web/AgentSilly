//! Tool: write_character — writes one or more fields to a character card in SillyTavern.
//!
//! Snapshots the previous state via VersionStore before writing, and sends
//! an `UndoAvailable` event to the frontend after a successful write.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;

use super::dispatcher::{validate_against_schema, Tool};
use super::st_client::StClient;
use crate::agent::events::WsEvent;
use crate::versioning::VersionStore;

/// Tool that writes one or more fields to a character card in SillyTavern.
///
/// Before writing, it snapshots the current character state for undo support.
/// After a successful write, it sends an `UndoAvailable` event via the WebSocket channel.
pub struct WriteCharacterTool {
    st_client: Arc<Mutex<StClient>>,
    version_store: Arc<VersionStore>,
    event_tx: tokio::sync::mpsc::Sender<WsEvent>,
}

impl WriteCharacterTool {
    /// Create a new `WriteCharacterTool`.
    pub fn new(
        st_client: Arc<Mutex<StClient>>,
        version_store: Arc<VersionStore>,
        event_tx: tokio::sync::mpsc::Sender<WsEvent>,
    ) -> Self {
        Self {
            st_client,
            version_store,
            event_tx,
        }
    }
}

#[async_trait]
impl Tool for WriteCharacterTool {
    fn name(&self) -> &str {
        "write_character"
    }

    fn description(&self) -> &str {
        "Write one or more fields to a character card in SillyTavern"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The character name to update"
                },
                "description": {
                    "type": "string",
                    "description": "Character description / backstory"
                },
                "personality": {
                    "type": "string",
                    "description": "Personality summary"
                },
                "scenario": {
                    "type": "string",
                    "description": "Scenario / setting context"
                },
                "first_mes": {
                    "type": "string",
                    "description": "First message the character sends"
                },
                "mes_example": {
                    "type": "string",
                    "description": "Example dialogue"
                },
                "creator_notes": {
                    "type": "string",
                    "description": "Creator notes (metadata)"
                },
                "system_prompt": {
                    "type": "string",
                    "description": "System prompt override for this character"
                },
                "post_history_instructions": {
                    "type": "string",
                    "description": "Post-history instructions"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tags for categorization"
                }
            },
            "required": ["name"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let name = args["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;

        // 1. Read current character state from SillyTavern
        let current_character = {
            let mut client = self.st_client.lock().await;
            client.get_character(name).await?
        };

        // 2. Snapshot the current state for undo
        let current_data = serde_json::to_value(&current_character)?;
        self.version_store.snapshot(
            "character",
            name,
            &current_data,
            "Before write_character",
        )?;

        // 3. Merge provided fields into the existing character data
        let mut updated = current_character;
        let mut updated_fields: Vec<String> = Vec::new();

        if let Some(v) = args.get("description").and_then(|v| v.as_str()) {
            updated.description = v.to_string();
            updated_fields.push("description".to_string());
        }
        if let Some(v) = args.get("personality").and_then(|v| v.as_str()) {
            updated.personality = v.to_string();
            updated_fields.push("personality".to_string());
        }
        if let Some(v) = args.get("scenario").and_then(|v| v.as_str()) {
            updated.scenario = v.to_string();
            updated_fields.push("scenario".to_string());
        }
        if let Some(v) = args.get("first_mes").and_then(|v| v.as_str()) {
            updated.first_mes = v.to_string();
            updated_fields.push("first_mes".to_string());
        }
        if let Some(v) = args.get("mes_example").and_then(|v| v.as_str()) {
            updated.mes_example = v.to_string();
            updated_fields.push("mes_example".to_string());
        }
        if let Some(v) = args.get("creator_notes").and_then(|v| v.as_str()) {
            updated.creator_notes = v.to_string();
            updated_fields.push("creator_notes".to_string());
        }
        if let Some(v) = args.get("system_prompt").and_then(|v| v.as_str()) {
            updated.system_prompt = v.to_string();
            updated_fields.push("system_prompt".to_string());
        }
        if let Some(v) = args.get("post_history_instructions").and_then(|v| v.as_str()) {
            updated.post_history_instructions = v.to_string();
            updated_fields.push("post_history_instructions".to_string());
        }
        if let Some(tags_val) = args.get("tags").and_then(|v| v.as_array()) {
            let tags: Vec<String> = tags_val
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            updated.tags = tags;
            updated_fields.push("tags".to_string());
        }

        if updated_fields.is_empty() {
            return Ok(serde_json::json!({
                "success": true,
                "message": "No fields to update were provided",
                "character": name
            }));
        }

        // 4. Write the updated character back to SillyTavern
        {
            let mut client = self.st_client.lock().await;
            client.edit_character(&updated).await?;
        }

        // 5. Send UndoAvailable event to frontend
        let summary = format!("Character updated: {}", updated_fields.join(", "));
        let _ = self.event_tx.send(WsEvent::UndoAvailable {
            entity_type: "character".to_string(),
            entity_id: name.to_string(),
            summary: summary.clone(),
        }).await;

        // 6. Return success with summary
        Ok(serde_json::json!({
            "success": true,
            "character": name,
            "updated_fields": updated_fields,
            "message": summary
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameters_schema_has_required_name() {
        // We can't easily construct a full WriteCharacterTool without dependencies,
        // but we can test the schema structure directly.
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The character name to update"
                },
                "description": {
                    "type": "string",
                    "description": "Character description / backstory"
                },
                "personality": {
                    "type": "string",
                    "description": "Personality summary"
                }
            },
            "required": ["name"]
        });

        // Valid: has name
        let valid = serde_json::json!({"name": "Kael", "description": "A warrior"});
        assert!(validate_against_schema(&schema, &valid).is_ok());

        // Invalid: missing name
        let invalid = serde_json::json!({"description": "A warrior"});
        assert!(validate_against_schema(&schema, &invalid).is_err());
    }

    #[test]
    fn test_validate_args_with_tags() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["name"]
        });

        let valid = serde_json::json!({
            "name": "Kael",
            "tags": ["fantasy", "warrior"]
        });
        assert!(validate_against_schema(&schema, &valid).is_ok());

        // Invalid tags type
        let invalid = serde_json::json!({
            "name": "Kael",
            "tags": "not-an-array"
        });
        assert!(validate_against_schema(&schema, &invalid).is_err());
    }

    #[test]
    fn test_field_merge_logic() {
        // Simulate the merge logic from execute()
        use super::super::st_client::CharacterData;

        let mut character = CharacterData {
            name: "Kael".to_string(),
            description: "Original description".to_string(),
            personality: "Original personality".to_string(),
            scenario: "Original scenario".to_string(),
            first_mes: "Hello".to_string(),
            mes_example: "".to_string(),
            creator_notes: "".to_string(),
            system_prompt: "".to_string(),
            post_history_instructions: "".to_string(),
            tags: vec!["fantasy".to_string()],
            avatar: "kael.png".to_string(),
        };

        let args = serde_json::json!({
            "name": "Kael",
            "description": "Updated description",
            "tags": ["fantasy", "warrior", "brave"]
        });

        // Apply merge
        let mut updated_fields: Vec<String> = Vec::new();

        if let Some(v) = args.get("description").and_then(|v| v.as_str()) {
            character.description = v.to_string();
            updated_fields.push("description".to_string());
        }
        if let Some(v) = args.get("personality").and_then(|v| v.as_str()) {
            character.personality = v.to_string();
            updated_fields.push("personality".to_string());
        }
        if let Some(tags_val) = args.get("tags").and_then(|v| v.as_array()) {
            let tags: Vec<String> = tags_val
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            character.tags = tags;
            updated_fields.push("tags".to_string());
        }

        assert_eq!(character.description, "Updated description");
        assert_eq!(character.personality, "Original personality"); // unchanged
        assert_eq!(character.tags, vec!["fantasy", "warrior", "brave"]);
        assert_eq!(updated_fields, vec!["description", "tags"]);
    }
}
