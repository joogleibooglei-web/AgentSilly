//! Tools: update_character and create_character — manage character cards in SillyTavern.
//!
//! `update_character` updates fields on an existing character card.
//! `create_character` creates a brand-new character card from scratch.
//!
//! Both snapshot the previous state via VersionStore before writing, and send
//! an `UndoAvailable` event to the frontend after a successful write.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;

use super::dispatcher::{validate_against_schema, Tool};
use super::draft_file::str_replace_first;
use super::st_client::{CharacterData, StClient};
use crate::agent::events::WsEvent;
use crate::agent::session::SharedSessionContext;
use crate::versioning::VersionStore;

// ─── UpdateCharacterTool ────────────────────────────────────────────────────

/// Tool that updates one or more fields on an existing character card in SillyTavern.
///
/// Before writing, it snapshots the current character state for undo support.
/// After a successful write, it sends an `UndoAvailable` event via the WebSocket channel.
pub struct UpdateCharacterTool {
    st_client: Arc<Mutex<StClient>>,
    version_store: Arc<VersionStore>,
    event_tx: tokio::sync::mpsc::Sender<WsEvent>,
    session_ctx: SharedSessionContext,
}

impl UpdateCharacterTool {
    /// Create a new `UpdateCharacterTool`.
    pub fn new(
        st_client: Arc<Mutex<StClient>>,
        version_store: Arc<VersionStore>,
        event_tx: tokio::sync::mpsc::Sender<WsEvent>,
        session_ctx: SharedSessionContext,
    ) -> Self {
        Self {
            st_client,
            version_store,
            event_tx,
            session_ctx,
        }
    }
}

#[async_trait]
impl Tool for UpdateCharacterTool {
    fn name(&self) -> &str {
        "update_character"
    }

    fn description(&self) -> &str {
        "Update one or more fields on an existing character card in SillyTavern. The character must already exist."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The character name to update (used for lookup)"
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
                },
                "replacements": {
                    "type": "array",
                    "description": "Find-and-replace edits on existing field content. Each entry targets a specific field and replaces the first occurrence of old_text with new_text. Use this instead of rewriting an entire field when you only need to change a small part.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "field": {
                                "type": "string",
                                "description": "The character field to perform the replacement on (e.g. description, personality, scenario, first_mes, mes_example, creator_notes, system_prompt, post_history_instructions)"
                            },
                            "old_text": {
                                "type": "string",
                                "description": "The text to find in the field's current content"
                            },
                            "new_text": {
                                "type": "string",
                                "description": "The replacement text"
                            }
                        },
                        "required": ["field", "old_text", "new_text"]
                    }
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
            "Before update_character",
        )?;

        // 4. Build the update payload (only changed fields)
        //
        // SillyTavern character cards use a dual-layer structure (TavernCard V2):
        //   - Top-level V1 fields: `description`, `personality`, `first_mes`, etc.
        //   - Nested V2 `data` object: `data.description`, `data.personality`, etc.
        //
        // When ST reads a card back (`readFromV2`), it copies `data.*` values
        // over the top-level fields — meaning `data.*` takes precedence.
        // The `merge-attributes` endpoint does a deep merge, so we MUST set
        // both the top-level field AND the corresponding `data.*` field for
        // any V2-spec field. Otherwise the stale `data.*` value overwrites
        // our top-level update on the next read.
        let mut updates = serde_json::Map::new();
        let mut data_updates = serde_json::Map::new();
        let mut updated_fields: Vec<String> = Vec::new();

        if let Some(v) = args.get("description").and_then(|v| v.as_str()) {
            let val = serde_json::Value::String(v.to_string());
            updates.insert("description".to_string(), val.clone());
            data_updates.insert("description".to_string(), val);
            updated_fields.push("description".to_string());
        }
        if let Some(v) = args.get("personality").and_then(|v| v.as_str()) {
            let val = serde_json::Value::String(v.to_string());
            updates.insert("personality".to_string(), val.clone());
            data_updates.insert("personality".to_string(), val);
            updated_fields.push("personality".to_string());
        }
        if let Some(v) = args.get("scenario").and_then(|v| v.as_str()) {
            let val = serde_json::Value::String(v.to_string());
            updates.insert("scenario".to_string(), val.clone());
            data_updates.insert("scenario".to_string(), val);
            updated_fields.push("scenario".to_string());
        }
        if let Some(v) = args.get("first_mes").and_then(|v| v.as_str()) {
            let val = serde_json::Value::String(v.to_string());
            updates.insert("first_mes".to_string(), val.clone());
            data_updates.insert("first_mes".to_string(), val);
            updated_fields.push("first_mes".to_string());
        }
        if let Some(v) = args.get("mes_example").and_then(|v| v.as_str()) {
            let val = serde_json::Value::String(v.to_string());
            updates.insert("mes_example".to_string(), val.clone());
            data_updates.insert("mes_example".to_string(), val);
            updated_fields.push("mes_example".to_string());
        }
        if let Some(v) = args.get("creator_notes").and_then(|v| v.as_str()) {
            let val = serde_json::Value::String(v.to_string());
            updates.insert("creator_notes".to_string(), val.clone());
            data_updates.insert("creator_notes".to_string(), val);
            updated_fields.push("creator_notes".to_string());
        }
        if let Some(v) = args.get("system_prompt").and_then(|v| v.as_str()) {
            let val = serde_json::Value::String(v.to_string());
            updates.insert("system_prompt".to_string(), val.clone());
            data_updates.insert("system_prompt".to_string(), val);
            updated_fields.push("system_prompt".to_string());
        }
        if let Some(v) = args.get("post_history_instructions").and_then(|v| v.as_str()) {
            let val = serde_json::Value::String(v.to_string());
            updates.insert("post_history_instructions".to_string(), val.clone());
            data_updates.insert("post_history_instructions".to_string(), val);
            updated_fields.push("post_history_instructions".to_string());
        }
        if let Some(tags_val) = args.get("tags").and_then(|v| v.as_array()) {
            let val = serde_json::Value::Array(tags_val.clone());
            updates.insert("tags".to_string(), val.clone());
            data_updates.insert("tags".to_string(), val);
            updated_fields.push("tags".to_string());
        }
        if let Some(alt_greetings_val) = args.get("alternate_greetings").and_then(|v| v.as_array()) {
            let val = serde_json::Value::Array(alt_greetings_val.clone());
            updates.insert("alternate_greetings".to_string(), val.clone());
            data_updates.insert("alternate_greetings".to_string(), val);
            updated_fields.push("alternate_greetings".to_string());
        }
        if let Some(v) = args.get("character_book") {
            if v.is_object() {
                updates.insert("character_book".to_string(), v.clone());
                data_updates.insert("character_book".to_string(), v.clone());
                updated_fields.push("character_book".to_string());
            }
        }
        if let Some(v) = args.get("extensions") {
            if v.is_object() {
                updates.insert("extensions".to_string(), v.clone());
                data_updates.insert("extensions".to_string(), v.clone());
                updated_fields.push("extensions".to_string());
            }
        }
        if let Some(v) = args.get("creator").and_then(|v| v.as_str()) {
            let val = serde_json::Value::String(v.to_string());
            updates.insert("creator".to_string(), val.clone());
            data_updates.insert("creator".to_string(), val);
            updated_fields.push("creator".to_string());
        }
        if let Some(v) = args.get("character_version").and_then(|v| v.as_str()) {
            let val = serde_json::Value::String(v.to_string());
            updates.insert("character_version".to_string(), val.clone());
            data_updates.insert("character_version".to_string(), val);
            updated_fields.push("character_version".to_string());
        }
        if let Some(v) = args.get("talkativeness").and_then(|v| v.as_f64()) {
            let val = serde_json::json!(v);
            updates.insert("talkativeness".to_string(), val.clone());
            // In V2, talkativeness lives under data.extensions.talkativeness
            let extensions_obj = data_updates
                .entry("extensions".to_string())
                .or_insert_with(|| serde_json::json!({}));
            if let Some(ext_map) = extensions_obj.as_object_mut() {
                ext_map.insert("talkativeness".to_string(), val);
            }
            updated_fields.push("talkativeness".to_string());
        }

        // 4b. Process find-and-replace edits on existing field content
        if let Some(replacements) = args.get("replacements").and_then(|v| v.as_array()) {
            for (i, replacement) in replacements.iter().enumerate() {
                let field = replacement
                    .get("field")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        anyhow::anyhow!("replacements[{}]: missing required parameter 'field'", i)
                    })?;
                let old_text = replacement
                    .get("old_text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "replacements[{}]: missing required parameter 'old_text'",
                            i
                        )
                    })?;
                let new_text = replacement
                    .get("new_text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "replacements[{}]: missing required parameter 'new_text'",
                            i
                        )
                    })?;

                // Get the current value of the field from the character
                let current_value = match field {
                    "description" => current_character.description.clone(),
                    "personality" => current_character.personality.clone(),
                    "scenario" => current_character.scenario.clone(),
                    "first_mes" => current_character.first_mes.clone(),
                    "mes_example" => current_character.mes_example.clone(),
                    "creator_notes" => current_character.creator_notes.clone(),
                    "system_prompt" => current_character.system_prompt.clone(),
                    "post_history_instructions" => {
                        current_character.post_history_instructions.clone()
                    }
                    "creator" => current_character.creator.clone(),
                    "character_version" => current_character.character_version.clone(),
                    _ => {
                        anyhow::bail!(
                            "replacements[{}]: unsupported field '{}'. Supported fields: description, personality, scenario, first_mes, mes_example, creator_notes, system_prompt, post_history_instructions, creator, character_version",
                            i,
                            field
                        );
                    }
                };

                // If a previous replacement in this batch already updated the same field,
                // use that value instead of the original
                let working_value = updates
                    .get(field)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or(current_value);

                let replaced = str_replace_first(&working_value, old_text, new_text).ok_or_else(
                    || {
                        anyhow::anyhow!(
                            "replacements[{}]: old_text not found in field '{}'. The text does not match any content in the current field value.",
                            i,
                            field
                        )
                    },
                )?;

                let val = serde_json::Value::String(replaced);
                updates.insert(field.to_string(), val.clone());
                data_updates.insert(field.to_string(), val);
                if !updated_fields.contains(&format!("{} (find/replace)", field)) {
                    updated_fields.push(format!("{} (find/replace)", field));
                }
            }
        }

        // Nest the V2 `data` object into the update payload
        if !data_updates.is_empty() {
            updates.insert("data".to_string(), serde_json::Value::Object(data_updates));
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

        // 5a. Update session context with the avatar_url of the character we just wrote
        {
            let mut ctx = self.session_ctx.lock().await;
            ctx.last_avatar_url = Some(avatar_url.clone());
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

// ─── CreateCharacterTool ────────────────────────────────────────────────────

/// Tool that creates a brand-new character card in SillyTavern.
///
/// After creation, it sends a preview event to the frontend.
pub struct CreateCharacterTool {
    st_client: Arc<Mutex<StClient>>,
    event_tx: tokio::sync::mpsc::Sender<WsEvent>,
    session_ctx: SharedSessionContext,
}

impl CreateCharacterTool {
    /// Create a new `CreateCharacterTool`.
    pub fn new(
        st_client: Arc<Mutex<StClient>>,
        event_tx: tokio::sync::mpsc::Sender<WsEvent>,
        session_ctx: SharedSessionContext,
    ) -> Self {
        Self {
            st_client,
            event_tx,
            session_ctx,
        }
    }
}

#[async_trait]
impl Tool for CreateCharacterTool {
    fn name(&self) -> &str {
        "create_character"
    }

    fn description(&self) -> &str {
        "Create a new character card in SillyTavern. The character must not already exist."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The character name (required, must be unique)"
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

        // 1. Check that the character doesn't already exist
        {
            let mut client = self.st_client.lock().await;
            let characters = client.get_characters().await?;
            if characters.iter().any(|c| c.name.eq_ignore_ascii_case(name)) {
                anyhow::bail!(
                    "Character '{}' already exists. Use update_character to modify it.",
                    name
                );
            }
        }

        // 2. Build the CharacterData from provided fields
        let character_data = CharacterData {
            name: name.to_string(),
            description: args.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            personality: args.get("personality").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            scenario: args.get("scenario").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            first_mes: args.get("first_mes").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            mes_example: args.get("mes_example").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            creator_notes: args.get("creator_notes").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            system_prompt: args.get("system_prompt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            post_history_instructions: args.get("post_history_instructions").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            tags: args.get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            avatar: String::new(), // ST assigns the avatar filename on creation
            alternate_greetings: args.get("alternate_greetings")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            character_book: args.get("character_book").and_then(|v| if v.is_object() { Some(v.clone()) } else { None }),
            extensions: args.get("extensions").and_then(|v| if v.is_object() { Some(v.clone()) } else { None }),
            creator: args.get("creator").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            character_version: args.get("character_version").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            talkativeness: args.get("talkativeness").and_then(|v| v.as_f64()),
        };

        // 3. Create the character in SillyTavern
        {
            let mut client = self.st_client.lock().await;
            client.create_character(&character_data).await?;
        }

        // 4. Re-read the created character to get the assigned avatar_url and send preview
        let avatar_url = {
            let mut client = self.st_client.lock().await;
            let characters = client.get_characters().await?;
            characters
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
                .map(|c| c.avatar.clone())
                .unwrap_or_default()
        };

        if !avatar_url.is_empty() {
            // Update session context
            {
                let mut ctx = self.session_ctx.lock().await;
                ctx.last_avatar_url = Some(avatar_url.clone());
            }

            // Send preview event
            let created_character = {
                let mut client = self.st_client.lock().await;
                client.get_character(&avatar_url).await?
            };
            let created_data = serde_json::to_value(&created_character)?;
            let _ = self.event_tx.send(WsEvent::Preview {
                tab: "character".to_string(),
                data: created_data,
            }).await;
        }

        // 5. Return success
        Ok(serde_json::json!({
            "success": true,
            "character": name,
            "avatar": avatar_url,
            "message": format!("Character '{}' created", name)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameters_schema_has_required_name() {
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
