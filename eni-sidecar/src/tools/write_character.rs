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
                },
                "alternate_greetings": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Alternate first messages the user can swap between"
                },
                "character_book": {
                    "type": "object",
                    "description": "Embedded lorebook / character book"
                },
                "extensions": {
                    "type": "object",
                    "description": "Freeform extension data (depth prompts, ST plugins, etc.)"
                },
                "creator": {
                    "type": "string",
                    "description": "Card creator attribution"
                },
                "character_version": {
                    "type": "string",
                    "description": "Version string for the card"
                },
                "talkativeness": {
                    "type": "number",
                    "description": "0.0-1.0 scale for how often the character initiates in group chats"
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

        // 1. Resolve character name to avatar_url
        let avatar_url = {
            let mut client = self.st_client.lock().await;
            let characters = client.get_characters().await?;
            characters
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
                .map(|c| c.avatar.clone())
                .ok_or_else(|| anyhow::anyhow!("Character '{}' not found", name))?
        };

        // 2. Read current character state for undo snapshot
        let current_character = {
            let mut client = self.st_client.lock().await;
            client.get_character(&avatar_url).await?
        };

        // 3. Snapshot the current state for undo
        let current_data = serde_json::to_value(&current_character)?;
        self.version_store.snapshot(
            "character",
            name,
            &current_data,
            "Before write_character",
        )?;

        // 4. Build the update payload (only changed fields)
        let mut updates = serde_json::Map::new();
        let mut updated_fields: Vec<String> = Vec::new();

        if let Some(v) = args.get("description").and_then(|v| v.as_str()) {
            updates.insert("description".to_string(), serde_json::Value::String(v.to_string()));
            updated_fields.push("description".to_string());
        }
        if let Some(v) = args.get("personality").and_then(|v| v.as_str()) {
            updates.insert("personality".to_string(), serde_json::Value::String(v.to_string()));
            updated_fields.push("personality".to_string());
        }
        if let Some(v) = args.get("scenario").and_then(|v| v.as_str()) {
            updates.insert("scenario".to_string(), serde_json::Value::String(v.to_string()));
            updated_fields.push("scenario".to_string());
        }
        if let Some(v) = args.get("first_mes").and_then(|v| v.as_str()) {
            updates.insert("first_mes".to_string(), serde_json::Value::String(v.to_string()));
            updated_fields.push("first_mes".to_string());
        }
        if let Some(v) = args.get("mes_example").and_then(|v| v.as_str()) {
            updates.insert("mes_example".to_string(), serde_json::Value::String(v.to_string()));
            updated_fields.push("mes_example".to_string());
        }
        if let Some(v) = args.get("creator_notes").and_then(|v| v.as_str()) {
            updates.insert("creator_notes".to_string(), serde_json::Value::String(v.to_string()));
            updated_fields.push("creator_notes".to_string());
        }
        if let Some(v) = args.get("system_prompt").and_then(|v| v.as_str()) {
            updates.insert("system_prompt".to_string(), serde_json::Value::String(v.to_string()));
            updated_fields.push("system_prompt".to_string());
        }
        if let Some(v) = args.get("post_history_instructions").and_then(|v| v.as_str()) {
            updates.insert("post_history_instructions".to_string(), serde_json::Value::String(v.to_string()));
            updated_fields.push("post_history_instructions".to_string());
        }
        if let Some(tags_val) = args.get("tags").and_then(|v| v.as_array()) {
            updates.insert("tags".to_string(), serde_json::Value::Array(tags_val.clone()));
            updated_fields.push("tags".to_string());
        }
        if let Some(alt_greetings_val) = args.get("alternate_greetings").and_then(|v| v.as_array()) {
            updates.insert("alternate_greetings".to_string(), serde_json::Value::Array(alt_greetings_val.clone()));
            updated_fields.push("alternate_greetings".to_string());
        }
        if let Some(v) = args.get("character_book") {
            if v.is_object() {
                updates.insert("character_book".to_string(), v.clone());
                updated_fields.push("character_book".to_string());
            }
        }
        if let Some(v) = args.get("extensions") {
            if v.is_object() {
                updates.insert("extensions".to_string(), v.clone());
                updated_fields.push("extensions".to_string());
            }
        }
        if let Some(v) = args.get("creator").and_then(|v| v.as_str()) {
            updates.insert("creator".to_string(), serde_json::Value::String(v.to_string()));
            updated_fields.push("creator".to_string());
        }
        if let Some(v) = args.get("character_version").and_then(|v| v.as_str()) {
            updates.insert("character_version".to_string(), serde_json::Value::String(v.to_string()));
            updated_fields.push("character_version".to_string());
        }
        if let Some(v) = args.get("talkativeness").and_then(|v| v.as_f64()) {
            updates.insert("talkativeness".to_string(), serde_json::json!(v));
            updated_fields.push("talkativeness".to_string());
        }

        if updated_fields.is_empty() {
            return Ok(serde_json::json!({
                "success": true,
                "message": "No fields to update were provided",
                "character": name
            }));
        }

        // 5. Write the updates to SillyTavern via merge-attributes
        {
            let update_value = serde_json::Value::Object(updates);
            let mut client = self.st_client.lock().await;
            client.edit_character(&avatar_url, &update_value).await?;
        }

        // 6. Re-read the updated character and send preview event to frontend
        let updated_character = {
            let mut client = self.st_client.lock().await;
            client.get_character(&avatar_url).await?
        };
        let updated_data = serde_json::to_value(&updated_character)?;
        let _ = self.event_tx.send(WsEvent::Preview {
            tab: "character".to_string(),
            data: updated_data,
        }).await;

        // 7. Send UndoAvailable event to frontend
        let summary = format!("Character updated: {}", updated_fields.join(", "));
        let _ = self.event_tx.send(WsEvent::UndoAvailable {
            entity_type: "character".to_string(),
            entity_id: name.to_string(),
            summary: summary.clone(),
        }).await;

        // 8. Return success with summary
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
            alternate_greetings: vec![],
            character_book: None,
            extensions: None,
            creator: "".to_string(),
            character_version: "".to_string(),
            talkativeness: None,
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
