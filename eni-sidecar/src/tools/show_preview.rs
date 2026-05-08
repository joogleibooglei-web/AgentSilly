//! Tool: show_preview — sends rendered content to the frontend for display in the Preview Pane.
//!
//! This tool performs no file I/O. It simply relays a data payload to the
//! Svelte frontend via a WebSocket `preview` event, causing the right pane
//! to open and display the content in the appropriate tab.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::dispatcher::{validate_against_schema, Tool};
use crate::agent::events::WsEvent;

/// Tool that sends a preview event to the frontend.
///
/// The frontend will open the right pane (if not already open) and switch
/// to the appropriate tab based on `content_type`.
pub struct ShowPreviewTool {
    event_tx: tokio::sync::mpsc::Sender<WsEvent>,
}

impl ShowPreviewTool {
    /// Create a new `ShowPreviewTool`.
    pub fn new(event_tx: tokio::sync::mpsc::Sender<WsEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl Tool for ShowPreviewTool {
    fn name(&self) -> &str {
        "show_preview"
    }

    fn description(&self) -> &str {
        "Display content in the preview pane. Opens the right panel and shows the data in the appropriate tab (character, world, posthistory, or persona)."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content_type": {
                    "type": "string",
                    "description": "The type of content to preview, determines which tab opens",
                    "enum": ["character", "world", "posthistory", "persona"]
                },
                "data": {
                    "type": "object",
                    "description": "The content data to display in the preview pane. Structure depends on content_type."
                }
            },
            "required": ["content_type", "data"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let content_type = args["content_type"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: content_type"))?;

        let data = args
            .get("data")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: data"))?;

        debug!(content_type = %content_type, "Sending preview event to frontend");

        // Send the preview event via WebSocket
        self.event_tx
            .send(WsEvent::Preview {
                tab: content_type.to_string(),
                data: data.clone(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send preview event: {}", e))?;

        Ok(serde_json::json!({
            "success": true,
            "content_type": content_type,
            "message": format!("Preview displayed in '{}' tab", content_type)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_validation() {
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        let tool = ShowPreviewTool::new(tx);

        // Valid: has content_type and data
        let valid = serde_json::json!({
            "content_type": "character",
            "data": {
                "name": "Kael",
                "description": "A warrior"
            }
        });
        assert!(tool.validate_args(&valid).is_ok());

        // Valid: world type
        let valid_world = serde_json::json!({
            "content_type": "world",
            "data": {
                "entries": [{"label": "Dragon Lore", "content": "..."}]
            }
        });
        assert!(tool.validate_args(&valid_world).is_ok());

        // Invalid: missing content_type
        let no_type = serde_json::json!({
            "data": {"name": "Kael"}
        });
        assert!(tool.validate_args(&no_type).is_err());

        // Invalid: missing data
        let no_data = serde_json::json!({
            "content_type": "character"
        });
        assert!(tool.validate_args(&no_data).is_err());

        // Invalid: bad content_type enum
        let bad_type = serde_json::json!({
            "content_type": "invalid",
            "data": {}
        });
        assert!(tool.validate_args(&bad_type).is_err());
    }

    #[tokio::test]
    async fn test_execute_sends_preview_event() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let tool = ShowPreviewTool::new(tx);

        let args = serde_json::json!({
            "content_type": "character",
            "data": {
                "name": "Kael",
                "description": "A cyberpunk warrior"
            }
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["content_type"], "character");

        // Verify the event was sent
        let event = rx.recv().await.unwrap();
        match event {
            WsEvent::Preview { tab, data } => {
                assert_eq!(tab, "character");
                assert_eq!(data["name"], "Kael");
            }
            _ => panic!("Expected Preview event"),
        }
    }

    #[tokio::test]
    async fn test_execute_world_preview() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let tool = ShowPreviewTool::new(tx);

        let args = serde_json::json!({
            "content_type": "world",
            "data": {
                "entries": [
                    {"label": "Dragon Lore", "content": "Dragons are ancient."},
                    {"label": "Elf History", "content": "Elves are immortal."}
                ]
            }
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);

        let event = rx.recv().await.unwrap();
        match event {
            WsEvent::Preview { tab, data } => {
                assert_eq!(tab, "world");
                assert_eq!(data["entries"].as_array().unwrap().len(), 2);
            }
            _ => panic!("Expected Preview event"),
        }
    }
}
