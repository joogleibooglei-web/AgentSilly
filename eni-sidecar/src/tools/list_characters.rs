//! Tool: list_characters — lists all characters available in SillyTavern.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;

use super::dispatcher::{validate_against_schema, Tool};
use super::st_client::StClient;

/// Tool that lists all characters from SillyTavern via the ST REST API.
///
/// Returns each character's name, avatar, and last modified date.
pub struct ListCharactersTool {
    st_client: Arc<Mutex<StClient>>,
}

impl ListCharactersTool {
    /// Create a new `ListCharactersTool` with a shared ST client.
    pub fn new(st_client: Arc<Mutex<StClient>>) -> Self {
        Self { st_client }
    }
}

#[async_trait]
impl Tool for ListCharactersTool {
    fn name(&self) -> &str {
        "list_characters"
    }

    fn description(&self) -> &str {
        "List all characters available in SillyTavern with their name, avatar, and last modified date"
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
        let characters = {
            let mut client = self.st_client.lock().await;
            client.get_characters().await?
        };

        let result = serde_json::to_value(&characters)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_args_empty_object() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {}
        });
        let args = serde_json::json!({});
        assert!(validate_against_schema(&schema, &args).is_ok());
    }

    #[test]
    fn test_validate_args_with_extra_fields() {
        // Extra fields should still pass since there are no required properties
        let schema = serde_json::json!({
            "type": "object",
            "properties": {}
        });
        let args = serde_json::json!({"unexpected": "field"});
        assert!(validate_against_schema(&schema, &args).is_ok());
    }

    #[test]
    fn test_validate_args_non_object_fails() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {}
        });
        let args = serde_json::json!("not an object");
        assert!(validate_against_schema(&schema, &args).is_err());
    }
}
