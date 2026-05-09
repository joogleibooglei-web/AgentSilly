//! Tools: read_persona, write_persona, list_personas — manage user personas in SillyTavern.
//!
//! SillyTavern stores personas in user settings (`power_user.personas` and
//! `power_user.persona_descriptions`), keyed by avatar filename. These tools
//! abstract that away — the user interacts by persona name (and optionally title
//! to disambiguate personas with the same name).
//!
//! `write_persona` snapshots the previous state via VersionStore before writing,
//! and sends an `UndoAvailable` event to the frontend after a successful write.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;

use super::dispatcher::{validate_against_schema, Tool};
use super::st_client::StClient;
use crate::agent::events::WsEvent;
use crate::versioning::VersionStore;

// ─── ReadPersonaTool ────────────────────────────────────────────────────────

/// Tool that reads a user persona from SillyTavern.
///
/// Looks up the persona by name (and optionally title to disambiguate).
/// Automatically sends a preview event to the frontend so the Persona tab updates.
pub struct ReadPersonaTool {
    st_client: Arc<Mutex<StClient>>,
    event_tx: tokio::sync::mpsc::Sender<WsEvent>,
}

impl ReadPersonaTool {
    /// Create a new `ReadPersonaTool` with a shared ST client and event sender.
    pub fn new(st_client: Arc<Mutex<StClient>>, event_tx: tokio::sync::mpsc::Sender<WsEvent>) -> Self {
        Self { st_client, event_tx }
    }
}

#[async_trait]
impl Tool for ReadPersonaTool {
    fn name(&self) -> &str {
        "read_persona"
    }

    fn description(&self) -> &str {
        "Read a user persona from SillyTavern by name. If multiple personas share the same name, use the title parameter to disambiguate."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The persona name to look up"
                },
                "title": {
                    "type": "string",
                    "description": "Optional title to disambiguate personas with the same name"
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

        let title = args.get("title").and_then(|v| v.as_str());

        let persona = {
            let mut client = self.st_client.lock().await;
            client.find_persona_by_name(name, title).await?
        };

        let result = serde_json::to_value(&persona)?;

        // Send preview event to frontend so the Persona tab updates automatically
        let _ = self.event_tx
            .send(WsEvent::Preview {
                tab: "persona".to_string(),
                data: result.clone(),
            })
            .await;

        Ok(result)
    }
}

// ─── WritePersonaTool ───────────────────────────────────────────────────────

/// Tool that updates a user persona in SillyTavern.
///
/// Before writing, it snapshots the current persona state for undo support.
/// After a successful write, it sends an `UndoAvailable` event via the WebSocket channel.
pub struct WritePersonaTool {
    st_client: Arc<Mutex<StClient>>,
    version_store: Arc<VersionStore>,
    event_tx: tokio::sync::mpsc::Sender<WsEvent>,
}

impl WritePersonaTool {
    /// Create a new `WritePersonaTool`.
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
impl Tool for WritePersonaTool {
    fn name(&self) -> &str {
        "write_persona"
    }

    fn description(&self) -> &str {
        "Update a user persona in SillyTavern. Looks up by name (and optionally title to disambiguate). Only provided fields are updated."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The persona name to update (used for lookup)"
                },
                "title": {
                    "type": "string",
                    "description": "Optional title to disambiguate personas with the same name, or to set/update the title"
                },
                "description": {
                    "type": "string",
                    "description": "The persona description / content to set"
                },
                "new_name": {
                    "type": "string",
                    "description": "Optional new name to rename the persona to"
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

        let title_for_lookup = args.get("title").and_then(|v| v.as_str());

        // 1. Read current persona state from SillyTavern
        let current_persona = {
            let mut client = self.st_client.lock().await;
            client.find_persona_by_name(name, title_for_lookup).await?
        };

        // 2. Snapshot the current state for undo
        let current_data = serde_json::to_value(&current_persona)?;
        let entity_id = &current_persona.avatar;
        self.version_store.snapshot(
            "persona",
            entity_id,
            &current_data,
            "Before write_persona",
        )?;

        // 3. Merge provided fields into the existing persona data
        let mut updated = current_persona.clone();

        if let Some(v) = args.get("description").and_then(|v| v.as_str()) {
            updated.description = v.to_string();
        }
        if let Some(v) = args.get("new_name").and_then(|v| v.as_str()) {
            updated.name = v.to_string();
        }
        if let Some(v) = args.get("title").and_then(|v| v.as_str()) {
            updated.title = v.to_string();
        }

        // 4. Write the updated persona back to SillyTavern
        {
            let mut client = self.st_client.lock().await;
            client.edit_persona(&updated).await?;
        }

        // 5. Send UndoAvailable event to frontend
        let _ = self.event_tx.send(WsEvent::UndoAvailable {
            entity_type: "persona".to_string(),
            entity_id: entity_id.to_string(),
            summary: "Persona updated".to_string(),
        }).await;

        // 6. Send preview event so the Persona tab updates with the new data
        let updated_data = serde_json::to_value(&updated)?;
        let _ = self.event_tx.send(WsEvent::Preview {
            tab: "persona".to_string(),
            data: updated_data,
        }).await;

        // 7. Return success
        Ok(serde_json::json!({
            "success": true,
            "persona": updated.name,
            "avatar": updated.avatar,
            "title": updated.title,
            "message": format!("Persona '{}' updated", updated.name)
        }))
    }
}

// ─── ListPersonasTool ───────────────────────────────────────────────────────

/// Tool that lists all user personas from SillyTavern.
///
/// Returns name, avatar ID, and title for each persona.
pub struct ListPersonasTool {
    st_client: Arc<Mutex<StClient>>,
}

impl ListPersonasTool {
    /// Create a new `ListPersonasTool` with a shared ST client.
    pub fn new(st_client: Arc<Mutex<StClient>>) -> Self {
        Self { st_client }
    }
}

#[async_trait]
impl Tool for ListPersonasTool {
    fn name(&self) -> &str {
        "list_personas"
    }

    fn description(&self) -> &str {
        "List all available user personas in SillyTavern. Returns name, avatar ID, and title for each."
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
        let personas = {
            let mut client = self.st_client.lock().await;
            client.get_personas().await?
        };

        let result = serde_json::to_value(&personas)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_persona_schema_requires_name() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The persona name to look up"
                },
                "title": {
                    "type": "string",
                    "description": "Optional title to disambiguate personas with the same name"
                }
            },
            "required": ["name"]
        });

        let valid = serde_json::json!({"name": "Default"});
        assert!(validate_against_schema(&schema, &valid).is_ok());

        let valid_with_title = serde_json::json!({"name": "Default", "title": "Main"});
        assert!(validate_against_schema(&schema, &valid_with_title).is_ok());

        let invalid = serde_json::json!({});
        assert!(validate_against_schema(&schema, &invalid).is_err());
    }

    #[test]
    fn test_write_persona_schema_requires_name() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The persona name to update"
                },
                "title": {
                    "type": "string",
                    "description": "Optional title"
                },
                "description": {
                    "type": "string",
                    "description": "The persona description / content"
                },
                "new_name": {
                    "type": "string",
                    "description": "Optional new name"
                }
            },
            "required": ["name"]
        });

        let valid = serde_json::json!({"name": "Default", "description": "A brave adventurer"});
        assert!(validate_against_schema(&schema, &valid).is_ok());

        let valid_no_desc = serde_json::json!({"name": "Default"});
        assert!(validate_against_schema(&schema, &valid_no_desc).is_ok());

        let valid_with_title = serde_json::json!({"name": "Default", "title": "Alt", "description": "New desc"});
        assert!(validate_against_schema(&schema, &valid_with_title).is_ok());

        let invalid = serde_json::json!({"description": "No name"});
        assert!(validate_against_schema(&schema, &invalid).is_err());
    }

    #[test]
    fn test_list_personas_schema_accepts_empty() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {}
        });

        let args = serde_json::json!({});
        assert!(validate_against_schema(&schema, &args).is_ok());
    }

    #[test]
    fn test_list_personas_schema_rejects_non_object() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {}
        });

        let args = serde_json::json!("not an object");
        assert!(validate_against_schema(&schema, &args).is_err());
    }
}
