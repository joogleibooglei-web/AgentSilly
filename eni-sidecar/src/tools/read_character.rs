//! Tool: read_character — reads one or more fields from a character card in SillyTavern.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;

use super::dispatcher::{validate_against_schema, Tool};
use super::st_client::StClient;

/// Tool that reads character data from SillyTavern via the ST REST API.
///
/// Supports returning all fields or a filtered subset specified by the caller.
pub struct ReadCharacterTool {
    st_client: Arc<Mutex<StClient>>,
}

impl ReadCharacterTool {
    /// Create a new `ReadCharacterTool` with a shared ST client.
    pub fn new(st_client: Arc<Mutex<StClient>>) -> Self {
        Self { st_client }
    }
}

#[async_trait]
impl Tool for ReadCharacterTool {
    fn name(&self) -> &str {
        "read_character"
    }

    fn description(&self) -> &str {
        "Read one or more fields from a character card in SillyTavern"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The character name to look up"
                },
                "fields": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Specific fields to return (e.g. [\"description\", \"personality\"]). If omitted, all fields are returned."
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

        // Resolve character name to avatar_url by listing all characters
        let avatar_url = {
            let mut client = self.st_client.lock().await;
            let characters = client.get_characters().await?;
            characters
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
                .map(|c| c.avatar.clone())
                .ok_or_else(|| anyhow::anyhow!("Character '{}' not found", name))?
        };

        // Fetch full character data from SillyTavern using avatar_url
        let character = {
            let mut client = self.st_client.lock().await;
            client.get_character(&avatar_url).await?
        };

        // Serialize the full character to a JSON Value
        let full_value = serde_json::to_value(&character)?;

        // If fields filter is specified, return only those fields
        if let Some(fields_array) = args.get("fields").and_then(|f| f.as_array()) {
            let fields: Vec<&str> = fields_array
                .iter()
                .filter_map(|v| v.as_str())
                .collect();

            if fields.is_empty() {
                // Empty fields array — return all fields
                return Ok(full_value);
            }

            let full_obj = full_value
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("Character data is not a JSON object"))?;

            let mut filtered = serde_json::Map::new();
            for field in &fields {
                if let Some(value) = full_obj.get(*field) {
                    filtered.insert(field.to_string(), value.clone());
                }
            }

            Ok(Value::Object(filtered))
        } else {
            // No fields filter — return everything
            Ok(full_value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: validate args against the tool's schema without needing a real StClient.
    fn validate_with_schema(args: &Value) -> Result<()> {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The character name to look up"
                },
                "fields": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Specific fields to return"
                }
            },
            "required": ["name"]
        });
        validate_against_schema(&schema, args)
    }

    #[test]
    fn test_validate_args_valid() {
        let valid = serde_json::json!({"name": "Kael"});
        assert!(validate_with_schema(&valid).is_ok());

        let valid_with_fields =
            serde_json::json!({"name": "Kael", "fields": ["description", "personality"]});
        assert!(validate_with_schema(&valid_with_fields).is_ok());
    }

    #[test]
    fn test_validate_args_missing_name() {
        let invalid = serde_json::json!({"fields": ["description"]});
        assert!(validate_with_schema(&invalid).is_err());
    }

    #[test]
    fn test_validate_args_wrong_type() {
        let invalid = serde_json::json!({"name": 123});
        assert!(validate_with_schema(&invalid).is_err());
    }

    #[test]
    fn test_field_filtering_logic() {
        // Simulate what execute() does with field filtering
        let full_value = serde_json::json!({
            "name": "Kael",
            "description": "A brave warrior",
            "personality": "Bold and fearless",
            "scenario": "Medieval fantasy",
            "first_mes": "Hello traveler"
        });

        let fields = vec!["description", "personality"];
        let full_obj = full_value.as_object().unwrap();

        let mut filtered = serde_json::Map::new();
        for field in &fields {
            if let Some(value) = full_obj.get(*field) {
                filtered.insert(field.to_string(), value.clone());
            }
        }

        let result = Value::Object(filtered);
        assert_eq!(result["description"], "A brave warrior");
        assert_eq!(result["personality"], "Bold and fearless");
        assert!(result.get("scenario").is_none());
        assert!(result.get("first_mes").is_none());
    }
}
