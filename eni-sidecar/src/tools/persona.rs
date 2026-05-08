//! Tools: read_persona, write_persona, list_personas — manage user personas in SillyTavern.
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

/// Tool that reads a user persona from SillyTavern via the ST REST API.
pub struct ReadPersonaTool {
    st_client: Arc<Mutex<StClient>>,
}

impl ReadPersonaTool {
    /// Create a new `ReadPersonaTool` with a shared ST client.
    pub fn new(st_client: Arc<Mutex<StClient>>) -> Self {
        Self { st_client }
    }
}

#[async_trait]
impl Tool for ReadPersonaTool {
    fn name(&self) -> &str {
        "read_persona"
    }

    fn description(&self) -> &str {
        "Read the active user persona from SillyTavern"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The persona name to look up"
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

        let persona = {
            let mut client = self.st_client.lock().await;
            client.get_persona(name).await?
        };

        let result = serde_json::to_value(&persona)?;
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
        "Update a user persona in SillyTavern"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The persona name to update"
                },
                "description": {
                    "type": "string",
                    "description": "The persona description / content"
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

        // 1. Read current persona state from SillyTavern
        let current_persona = {
            let mut client = self.st_client.lock().await;
            client.get_persona(name).await?
        };

        // 2. Snapshot the current state for undo
        let current_data = serde_json::to_value(&current_persona)?;
        self.version_store.snapshot(
            "persona",
            name,
            &current_data,
            "Before write_persona",
        )?;

        // 3. Merge provided fields into the existing persona data
        let mut updated = current_persona;

        if let Some(v) = args.get("description").and_then(|v| v.as_str()) {
            updated.description = v.to_string();
        }

        // 4. Write the updated persona back to SillyTavern
        {
            let mut client = self.st_client.lock().await;
            client.edit_persona(&updated).await?;
        }

        // 5. Send UndoAvailable event to frontend
        let _ = self.event_tx.send(WsEvent::UndoAvailable {
            entity_type: "persona".to_string(),
            entity_id: name.to_string(),
            summary: "Persona updated".to_string(),
        }).await;

        // 6. Return success
        Ok(serde_json::json!({
            "success": true,
            "persona": name,
            "message": "Persona updated"
        }))
    }
}

// ─── ListPersonasTool ───────────────────────────────────────────────────────

/// Tool that lists all user personas from SillyTavern via the ST REST API.
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
        "List all available user personas in SillyTavern"
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
                }
            },
            "required": ["name"]
        });

        let valid = serde_json::json!({"name": "Default"});
        assert!(validate_against_schema(&schema, &valid).is_ok());

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
                "description": {
                    "type": "string",
                    "description": "The persona description / content"
                }
            },
            "required": ["name"]
        });

        let valid = serde_json::json!({"name": "Default", "description": "A brave adventurer"});
        assert!(validate_against_schema(&schema, &valid).is_ok());

        let valid_no_desc = serde_json::json!({"name": "Default"});
        assert!(validate_against_schema(&schema, &valid_no_desc).is_ok());

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
