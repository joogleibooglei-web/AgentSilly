//! Draft-based tools for world info and post-history workflows.
//!
//! These tools create, edit, and finalize ephemeral draft files stored at
//! `/tmp/eni-sidecar/`. Real-time preview is pushed to the frontend via
//! WebSocket `Preview` events.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tracing::debug;

use super::dispatcher::{validate_against_schema, Tool};
use super::draft_file::{delete_draft, read_draft, str_replace_first, write_draft, POST_HISTORY_DRAFT_PATH, WORLD_DRAFT_PATH};
use super::st_client::StClient;
use crate::agent::events::WsEvent;
use crate::agent::session::SharedSessionContext;

// ---------------------------------------------------------------------------
// CreateWorldDraftTool
// ---------------------------------------------------------------------------

/// Tool that creates a world info draft file at the fixed path.
///
/// Writes the provided content to `/tmp/eni-sidecar/world_draft.txt` and sends
/// a Preview event to the frontend. If a previous draft existed, includes a
/// warning in the response.
pub struct CreateWorldDraftTool {
    event_tx: mpsc::Sender<WsEvent>,
}

impl CreateWorldDraftTool {
    /// Create a new `CreateWorldDraftTool`.
    pub fn new(event_tx: mpsc::Sender<WsEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl Tool for CreateWorldDraftTool {
    fn name(&self) -> &str {
        "create_world_draft"
    }

    fn description(&self) -> &str {
        "Create a world info draft file. The content will be previewed in the World tab and can be edited or finalized later."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The world info text content to write to the draft file"
                }
            },
            "required": ["content"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: content"))?;

        // Write draft file (creates directory if needed, returns whether file existed)
        let existed = write_draft(WORLD_DRAFT_PATH, content).await?;

        debug!(
            path = WORLD_DRAFT_PATH,
            overwritten = existed,
            "World draft created"
        );

        // Send Preview event to frontend
        let _ = self
            .event_tx
            .send(WsEvent::Preview {
                tab: "world".to_string(),
                data: Value::String(content.to_string()),
            })
            .await;

        // Build response
        let mut response = serde_json::json!({
            "success": true,
            "path": WORLD_DRAFT_PATH,
        });

        if existed {
            response["warning"] = Value::String("Previous draft was replaced".to_string());
        }

        Ok(response)
    }
}

// ---------------------------------------------------------------------------
// EditWorldDraftTool
// ---------------------------------------------------------------------------

/// Tool that edits an existing world info draft using text replacement.
///
/// Reads the current draft from `/tmp/eni-sidecar/world_draft.txt`, replaces
/// the first occurrence of `old_text` with `new_text`, writes the result back,
/// and sends a Preview event to the frontend.
pub struct EditWorldDraftTool {
    event_tx: mpsc::Sender<WsEvent>,
}

impl EditWorldDraftTool {
    /// Create a new `EditWorldDraftTool`.
    pub fn new(event_tx: mpsc::Sender<WsEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl Tool for EditWorldDraftTool {
    fn name(&self) -> &str {
        "edit_world_draft"
    }

    fn description(&self) -> &str {
        "Edit an existing world info draft by replacing the first occurrence of old_text with new_text."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "old_text": {
                    "type": "string",
                    "description": "The text to find and replace in the current draft"
                },
                "new_text": {
                    "type": "string",
                    "description": "The replacement text"
                }
            },
            "required": ["old_text", "new_text"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let old_text = args["old_text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: old_text"))?;
        let new_text = args["new_text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: new_text"))?;

        // Read the current draft
        let content = read_draft(WORLD_DRAFT_PATH)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No draft exists at {}. Use create_world_draft first.",
                    WORLD_DRAFT_PATH
                )
            })?;

        // Replace first occurrence
        let new_content = str_replace_first(&content, old_text, new_text).ok_or_else(|| {
            anyhow::anyhow!(
                "Text not found in draft. The old_text does not match any content in the current draft."
            )
        })?;

        // Write the updated content back
        write_draft(WORLD_DRAFT_PATH, &new_content).await?;

        debug!(
            path = WORLD_DRAFT_PATH,
            "World draft edited"
        );

        // Send Preview event to frontend
        let _ = self
            .event_tx
            .send(WsEvent::Preview {
                tab: "world".to_string(),
                data: Value::String(new_content.clone()),
            })
            .await;

        Ok(serde_json::json!({
            "success": true,
            "path": WORLD_DRAFT_PATH,
        }))
    }
}

// ---------------------------------------------------------------------------
// FinalizeWorldInfoTool
// ---------------------------------------------------------------------------

/// Tool that finalizes the world info draft by prepending it to the character's
/// description field in SillyTavern.
///
/// Reads the draft from `/tmp/eni-sidecar/world_draft.txt`, fetches the current
/// character description, prepends the draft content with a `\n\n` separator,
/// writes the merged result back via StClient, deletes the draft file, and sends
/// a Preview event with null data to clear the frontend preview pane.
pub struct FinalizeWorldInfoTool {
    st_client: Arc<Mutex<StClient>>,
    session_ctx: SharedSessionContext,
    event_tx: mpsc::Sender<WsEvent>,
}

impl FinalizeWorldInfoTool {
    /// Create a new `FinalizeWorldInfoTool`.
    pub fn new(
        st_client: Arc<Mutex<StClient>>,
        session_ctx: SharedSessionContext,
        event_tx: mpsc::Sender<WsEvent>,
    ) -> Self {
        Self {
            st_client,
            session_ctx,
            event_tx,
        }
    }
}

#[async_trait]
impl Tool for FinalizeWorldInfoTool {
    fn name(&self) -> &str {
        "finalize_world_info"
    }

    fn description(&self) -> &str {
        "Finalize the world info draft by prepending it to the character's description field in SillyTavern."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, _args: Value) -> Result<Value> {
        // 1. Read draft — error if missing
        let draft_content = read_draft(WORLD_DRAFT_PATH)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No draft exists at {}. Use create_world_draft first.",
                    WORLD_DRAFT_PATH
                )
            })?;

        // 2. Read session context avatar — error if missing
        let avatar_url = {
            let ctx = self.session_ctx.lock().await;
            ctx.last_avatar_url.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "No target character in session. Use read_character first to select a character."
                )
            })?
        };

        // 3. Fetch current character via StClient
        let character = {
            let mut client = self.st_client.lock().await;
            client.get_character(&avatar_url).await?
        };

        // 4. Compute merged description: draft prepended to existing description
        let merged_description = format!("{}\n\n{}", draft_content, character.description);

        // 5. Write merged description back via StClient
        {
            let updates = serde_json::json!({ "description": merged_description });
            let mut client = self.st_client.lock().await;
            client.edit_character(&avatar_url, &updates).await?;
        }

        // 6. Delete draft file
        delete_draft(WORLD_DRAFT_PATH).await?;

        debug!(
            avatar = %avatar_url,
            "World info finalized and draft deleted"
        );

        // 7. Send Preview event with null data to clear the draft preview pane
        let _ = self
            .event_tx
            .send(WsEvent::Preview {
                tab: "world".to_string(),
                data: Value::Null,
            })
            .await;

        // 8. Re-read the updated character and send character preview to frontend
        let updated_character = {
            let mut client = self.st_client.lock().await;
            client.get_character(&avatar_url).await?
        };
        let updated_data = serde_json::to_value(&updated_character)?;
        let _ = self
            .event_tx
            .send(WsEvent::Preview {
                tab: "character".to_string(),
                data: updated_data,
            })
            .await;

        // 9. Return success response
        Ok(serde_json::json!({
            "success": true,
            "character": avatar_url,
            "message": "World info finalized"
        }))
    }
}

// ---------------------------------------------------------------------------
// CreatePostHistoryDraftTool
// ---------------------------------------------------------------------------

/// Tool that creates a post-history draft file at the fixed path.
///
/// Writes the provided content to `/tmp/eni-sidecar/post_history_draft.txt` and sends
/// a Preview event to the frontend. If a previous draft existed, includes a
/// warning in the response.
pub struct CreatePostHistoryDraftTool {
    event_tx: mpsc::Sender<WsEvent>,
}

impl CreatePostHistoryDraftTool {
    /// Create a new `CreatePostHistoryDraftTool`.
    pub fn new(event_tx: mpsc::Sender<WsEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl Tool for CreatePostHistoryDraftTool {
    fn name(&self) -> &str {
        "create_post_history_draft"
    }

    fn description(&self) -> &str {
        "Create a post-history instructions draft file. The content will be previewed in the Post-History tab and can be edited or finalized later."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The post-history instructions text content to write to the draft file"
                }
            },
            "required": ["content"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: content"))?;

        // Write draft file (creates directory if needed, returns whether file existed)
        let existed = write_draft(POST_HISTORY_DRAFT_PATH, content).await?;

        debug!(
            path = POST_HISTORY_DRAFT_PATH,
            overwritten = existed,
            "Post-history draft created"
        );

        // Send Preview event to frontend
        let _ = self
            .event_tx
            .send(WsEvent::Preview {
                tab: "posthistory".to_string(),
                data: Value::String(content.to_string()),
            })
            .await;

        // Build response
        let mut response = serde_json::json!({
            "success": true,
            "path": POST_HISTORY_DRAFT_PATH,
        });

        if existed {
            response["warning"] = Value::String("Previous draft was replaced".to_string());
        }

        Ok(response)
    }
}

// ---------------------------------------------------------------------------
// EditPostHistoryDraftTool
// ---------------------------------------------------------------------------

/// Tool that edits an existing post-history draft using text replacement.
///
/// Reads the current draft from `/tmp/eni-sidecar/post_history_draft.txt`, replaces
/// the first occurrence of `old_text` with `new_text`, writes the result back,
/// and sends a Preview event to the frontend.
pub struct EditPostHistoryDraftTool {
    event_tx: mpsc::Sender<WsEvent>,
}

impl EditPostHistoryDraftTool {
    /// Create a new `EditPostHistoryDraftTool`.
    pub fn new(event_tx: mpsc::Sender<WsEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl Tool for EditPostHistoryDraftTool {
    fn name(&self) -> &str {
        "edit_post_history_draft"
    }

    fn description(&self) -> &str {
        "Edit an existing post-history draft by replacing the first occurrence of old_text with new_text."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "old_text": {
                    "type": "string",
                    "description": "The text to find and replace in the current draft"
                },
                "new_text": {
                    "type": "string",
                    "description": "The replacement text"
                }
            },
            "required": ["old_text", "new_text"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let old_text = args["old_text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: old_text"))?;
        let new_text = args["new_text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: new_text"))?;

        // Read the current draft
        let content = read_draft(POST_HISTORY_DRAFT_PATH)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No draft exists at {}. Use create_post_history_draft first.",
                    POST_HISTORY_DRAFT_PATH
                )
            })?;

        // Replace first occurrence
        let new_content = str_replace_first(&content, old_text, new_text).ok_or_else(|| {
            anyhow::anyhow!(
                "Text not found in draft. The old_text does not match any content in the current draft."
            )
        })?;

        // Write the updated content back
        write_draft(POST_HISTORY_DRAFT_PATH, &new_content).await?;

        debug!(
            path = POST_HISTORY_DRAFT_PATH,
            "Post-history draft edited"
        );

        // Send Preview event to frontend
        let _ = self
            .event_tx
            .send(WsEvent::Preview {
                tab: "posthistory".to_string(),
                data: Value::String(new_content.clone()),
            })
            .await;

        Ok(serde_json::json!({
            "success": true,
            "path": POST_HISTORY_DRAFT_PATH,
        }))
    }
}

// ---------------------------------------------------------------------------
// FinalizePostHistoryTool
// ---------------------------------------------------------------------------

/// Tool that finalizes the post-history draft by writing it directly to the
/// character's `post_history_instructions` field in SillyTavern.
///
/// Unlike `FinalizeWorldInfoTool` which prepends to the description, this tool
/// performs a full replacement of the `post_history_instructions` field with the
/// draft content. After finalization, the draft file is deleted and a Preview
/// event with null data is sent to clear the frontend preview pane.
pub struct FinalizePostHistoryTool {
    st_client: Arc<Mutex<StClient>>,
    session_ctx: SharedSessionContext,
    event_tx: mpsc::Sender<WsEvent>,
}

impl FinalizePostHistoryTool {
    /// Create a new `FinalizePostHistoryTool`.
    pub fn new(
        st_client: Arc<Mutex<StClient>>,
        session_ctx: SharedSessionContext,
        event_tx: mpsc::Sender<WsEvent>,
    ) -> Self {
        Self {
            st_client,
            session_ctx,
            event_tx,
        }
    }
}

#[async_trait]
impl Tool for FinalizePostHistoryTool {
    fn name(&self) -> &str {
        "finalize_post_history"
    }

    fn description(&self) -> &str {
        "Finalize the post-history draft by writing it to the character's post_history_instructions field in SillyTavern (full replacement)."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, _args: Value) -> Result<Value> {
        // 1. Read draft — error if missing
        let draft_content = read_draft(POST_HISTORY_DRAFT_PATH)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No draft exists at {}. Use create_post_history_draft first.",
                    POST_HISTORY_DRAFT_PATH
                )
            })?;

        // 2. Read session context avatar — error if missing
        let avatar_url = {
            let ctx = self.session_ctx.lock().await;
            ctx.last_avatar_url.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "No target character in session. Use read_character first to select a character."
                )
            })?
        };

        // 3. Write draft content directly to post_history_instructions (full replacement)
        {
            let updates = serde_json::json!({ "post_history_instructions": draft_content });
            let mut client = self.st_client.lock().await;
            client.edit_character(&avatar_url, &updates).await?;
        }

        // 4. Delete draft file
        delete_draft(POST_HISTORY_DRAFT_PATH).await?;

        debug!(
            avatar = %avatar_url,
            "Post-history finalized and draft deleted"
        );

        // 5. Send Preview event with null data to clear the draft preview pane
        let _ = self
            .event_tx
            .send(WsEvent::Preview {
                tab: "posthistory".to_string(),
                data: Value::Null,
            })
            .await;

        // 6. Re-read the updated character and send character preview to frontend
        let updated_character = {
            let mut client = self.st_client.lock().await;
            client.get_character(&avatar_url).await?
        };
        let updated_data = serde_json::to_value(&updated_character)?;
        let _ = self
            .event_tx
            .send(WsEvent::Preview {
                tab: "character".to_string(),
                data: updated_data,
            })
            .await;

        // 7. Return success response
        Ok(serde_json::json!({
            "success": true,
            "character": avatar_url,
            "message": "Post-history finalized"
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_world_draft_tool_name() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = CreateWorldDraftTool::new(tx);
        assert_eq!(tool.name(), "create_world_draft");
    }

    #[test]
    fn test_create_world_draft_tool_schema() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = CreateWorldDraftTool::new(tx);
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["content"].is_object());
        assert_eq!(schema["properties"]["content"]["type"], "string");

        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&Value::String("content".to_string())));
    }

    #[test]
    fn test_create_world_draft_validate_args_valid() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = CreateWorldDraftTool::new(tx);

        let valid = serde_json::json!({"content": "Hello world info"});
        assert!(tool.validate_args(&valid).is_ok());
    }

    #[test]
    fn test_create_world_draft_validate_args_missing_content() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = CreateWorldDraftTool::new(tx);

        let invalid = serde_json::json!({});
        assert!(tool.validate_args(&invalid).is_err());
    }

    #[test]
    fn test_create_world_draft_validate_args_wrong_type() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = CreateWorldDraftTool::new(tx);

        let invalid = serde_json::json!({"content": 123});
        assert!(tool.validate_args(&invalid).is_err());
    }

    #[tokio::test]
    async fn test_create_world_draft_execute() {
        let (tx, mut rx) = mpsc::channel(16);
        let tool = CreateWorldDraftTool::new(tx);

        let args = serde_json::json!({"content": "Dragons roam the northern wastes."});
        let result = tool.execute(args).await.unwrap();

        assert_eq!(result["success"], true);
        assert_eq!(result["path"], WORLD_DRAFT_PATH);

        // Verify Preview event was sent
        let event = rx.recv().await.unwrap();
        match event {
            WsEvent::Preview { tab, data } => {
                assert_eq!(tab, "world");
                assert_eq!(data, Value::String("Dragons roam the northern wastes.".to_string()));
            }
            _ => panic!("Expected Preview event"),
        }
    }

    #[tokio::test]
    async fn test_create_world_draft_overwrite_warns() {
        let (tx, mut rx) = mpsc::channel(16);
        let tool = CreateWorldDraftTool::new(tx);

        // First create
        let args1 = serde_json::json!({"content": "First draft"});
        let result1 = tool.execute(args1).await.unwrap();
        assert_eq!(result1["success"], true);
        // Drain the first preview event
        let _ = rx.recv().await;

        // Second create (overwrite)
        let args2 = serde_json::json!({"content": "Second draft"});
        let result2 = tool.execute(args2).await.unwrap();
        assert_eq!(result2["success"], true);
        assert_eq!(result2["warning"], "Previous draft was replaced");

        // Verify second Preview event
        let event = rx.recv().await.unwrap();
        match event {
            WsEvent::Preview { tab, data } => {
                assert_eq!(tab, "world");
                assert_eq!(data, Value::String("Second draft".to_string()));
            }
            _ => panic!("Expected Preview event"),
        }
    }

    // -----------------------------------------------------------------------
    // CreatePostHistoryDraftTool tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_post_history_draft_tool_name() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = CreatePostHistoryDraftTool::new(tx);
        assert_eq!(tool.name(), "create_post_history_draft");
    }

    #[test]
    fn test_create_post_history_draft_tool_schema() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = CreatePostHistoryDraftTool::new(tx);
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["content"].is_object());
        assert_eq!(schema["properties"]["content"]["type"], "string");

        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&Value::String("content".to_string())));
    }

    #[test]
    fn test_create_post_history_draft_validate_args_valid() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = CreatePostHistoryDraftTool::new(tx);

        let valid = serde_json::json!({"content": "Write in third person."});
        assert!(tool.validate_args(&valid).is_ok());
    }

    #[test]
    fn test_create_post_history_draft_validate_args_missing_content() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = CreatePostHistoryDraftTool::new(tx);

        let invalid = serde_json::json!({});
        assert!(tool.validate_args(&invalid).is_err());
    }

    #[test]
    fn test_create_post_history_draft_validate_args_wrong_type() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = CreatePostHistoryDraftTool::new(tx);

        let invalid = serde_json::json!({"content": 42});
        assert!(tool.validate_args(&invalid).is_err());
    }

    #[tokio::test]
    async fn test_create_post_history_draft_execute() {
        let (tx, mut rx) = mpsc::channel(16);
        let tool = CreatePostHistoryDraftTool::new(tx);

        let args = serde_json::json!({"content": "Always end with an action hook."});
        let result = tool.execute(args).await.unwrap();

        assert_eq!(result["success"], true);
        assert_eq!(result["path"], POST_HISTORY_DRAFT_PATH);

        // Verify Preview event was sent
        let event = rx.recv().await.unwrap();
        match event {
            WsEvent::Preview { tab, data } => {
                assert_eq!(tab, "posthistory");
                assert_eq!(data, Value::String("Always end with an action hook.".to_string()));
            }
            _ => panic!("Expected Preview event"),
        }
    }

    #[tokio::test]
    async fn test_create_post_history_draft_overwrite_warns() {
        let (tx, mut rx) = mpsc::channel(16);
        let tool = CreatePostHistoryDraftTool::new(tx);

        // First create
        let args1 = serde_json::json!({"content": "First post-history draft"});
        let result1 = tool.execute(args1).await.unwrap();
        assert_eq!(result1["success"], true);
        // Drain the first preview event
        let _ = rx.recv().await;

        // Second create (overwrite)
        let args2 = serde_json::json!({"content": "Second post-history draft"});
        let result2 = tool.execute(args2).await.unwrap();
        assert_eq!(result2["success"], true);
        assert_eq!(result2["warning"], "Previous draft was replaced");

        // Verify second Preview event
        let event = rx.recv().await.unwrap();
        match event {
            WsEvent::Preview { tab, data } => {
                assert_eq!(tab, "posthistory");
                assert_eq!(data, Value::String("Second post-history draft".to_string()));
            }
            _ => panic!("Expected Preview event"),
        }
    }

    // -----------------------------------------------------------------------
    // EditWorldDraftTool tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_edit_world_draft_tool_name() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = EditWorldDraftTool::new(tx);
        assert_eq!(tool.name(), "edit_world_draft");
    }

    #[test]
    fn test_edit_world_draft_tool_schema() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = EditWorldDraftTool::new(tx);
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["old_text"].is_object());
        assert_eq!(schema["properties"]["old_text"]["type"], "string");
        assert!(schema["properties"]["new_text"].is_object());
        assert_eq!(schema["properties"]["new_text"]["type"], "string");

        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&Value::String("old_text".to_string())));
        assert!(required.contains(&Value::String("new_text".to_string())));
    }

    #[test]
    fn test_edit_world_draft_validate_args_valid() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = EditWorldDraftTool::new(tx);

        let valid = serde_json::json!({"old_text": "hello", "new_text": "world"});
        assert!(tool.validate_args(&valid).is_ok());
    }

    #[test]
    fn test_edit_world_draft_validate_args_missing_old_text() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = EditWorldDraftTool::new(tx);

        let invalid = serde_json::json!({"new_text": "world"});
        assert!(tool.validate_args(&invalid).is_err());
    }

    #[test]
    fn test_edit_world_draft_validate_args_missing_new_text() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = EditWorldDraftTool::new(tx);

        let invalid = serde_json::json!({"old_text": "hello"});
        assert!(tool.validate_args(&invalid).is_err());
    }

    #[test]
    fn test_edit_world_draft_validate_args_wrong_type() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = EditWorldDraftTool::new(tx);

        let invalid = serde_json::json!({"old_text": 123, "new_text": "world"});
        assert!(tool.validate_args(&invalid).is_err());
    }

    /// Tests edit success, missing draft error, and text-not-found error.
    /// Combined into one sequential test to avoid file system race conditions
    /// (all edit tests share the same WORLD_DRAFT_PATH).
    #[tokio::test]
    async fn test_edit_world_draft_execute() {
        use crate::tools::draft_file::delete_draft;

        let (tx, mut rx) = mpsc::channel(16);

        // --- Part 1: No draft exists error ---
        delete_draft(WORLD_DRAFT_PATH).await.unwrap();

        let edit_tool = EditWorldDraftTool::new(tx.clone());
        let args = serde_json::json!({"old_text": "hello", "new_text": "world"});
        let result = edit_tool.execute(args).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("No draft exists"),
            "Expected 'No draft exists' error, got: {}",
            err_msg
        );

        // --- Part 2: Successful edit ---
        write_draft(WORLD_DRAFT_PATH, "Dragons roam the northern wastes.")
            .await
            .unwrap();

        let edit_tool = EditWorldDraftTool::new(tx.clone());
        let edit_args = serde_json::json!({"old_text": "northern", "new_text": "southern"});
        let result = edit_tool.execute(edit_args).await.unwrap();

        assert_eq!(result["success"], true);
        assert_eq!(result["path"], WORLD_DRAFT_PATH);

        // Verify Preview event was sent with updated content
        let event = rx.recv().await.unwrap();
        match event {
            WsEvent::Preview { tab, data } => {
                assert_eq!(tab, "world");
                assert_eq!(
                    data,
                    Value::String("Dragons roam the southern wastes.".to_string())
                );
            }
            _ => panic!("Expected Preview event"),
        }

        // --- Part 3: Text not found error ---
        let edit_tool = EditWorldDraftTool::new(tx);
        let args = serde_json::json!({"old_text": "unicorns", "new_text": "griffins"});
        let result = edit_tool.execute(args).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Text not found in draft"),
            "Expected 'Text not found' error, got: {}",
            err_msg
        );
    }

    // -----------------------------------------------------------------------
    // EditPostHistoryDraftTool tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_edit_post_history_draft_tool_name() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = EditPostHistoryDraftTool::new(tx);
        assert_eq!(tool.name(), "edit_post_history_draft");
    }

    #[test]
    fn test_edit_post_history_draft_tool_schema() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = EditPostHistoryDraftTool::new(tx);
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["old_text"].is_object());
        assert_eq!(schema["properties"]["old_text"]["type"], "string");
        assert!(schema["properties"]["new_text"].is_object());
        assert_eq!(schema["properties"]["new_text"]["type"], "string");

        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&Value::String("old_text".to_string())));
        assert!(required.contains(&Value::String("new_text".to_string())));
    }

    #[test]
    fn test_edit_post_history_draft_validate_args_valid() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = EditPostHistoryDraftTool::new(tx);

        let valid = serde_json::json!({"old_text": "hello", "new_text": "world"});
        assert!(tool.validate_args(&valid).is_ok());
    }

    #[test]
    fn test_edit_post_history_draft_validate_args_missing_old_text() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = EditPostHistoryDraftTool::new(tx);

        let invalid = serde_json::json!({"new_text": "world"});
        assert!(tool.validate_args(&invalid).is_err());
    }

    #[test]
    fn test_edit_post_history_draft_validate_args_missing_new_text() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = EditPostHistoryDraftTool::new(tx);

        let invalid = serde_json::json!({"old_text": "hello"});
        assert!(tool.validate_args(&invalid).is_err());
    }

    #[test]
    fn test_edit_post_history_draft_validate_args_wrong_type() {
        let (tx, _rx) = mpsc::channel(16);
        let tool = EditPostHistoryDraftTool::new(tx);

        let invalid = serde_json::json!({"old_text": 123, "new_text": "world"});
        assert!(tool.validate_args(&invalid).is_err());
    }

    /// Tests edit success, missing draft error, and text-not-found error.
    /// Combined into one sequential test to avoid file system race conditions
    /// (all edit tests share the same POST_HISTORY_DRAFT_PATH).
    #[tokio::test]
    async fn test_edit_post_history_draft_execute() {
        use crate::tools::draft_file::delete_draft;

        let (tx, mut rx) = mpsc::channel(16);

        // --- Part 1: No draft exists error ---
        delete_draft(POST_HISTORY_DRAFT_PATH).await.unwrap();

        let edit_tool = EditPostHistoryDraftTool::new(tx.clone());
        let args = serde_json::json!({"old_text": "hello", "new_text": "world"});
        let result = edit_tool.execute(args).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("No draft exists"),
            "Expected 'No draft exists' error, got: {}",
            err_msg
        );

        // --- Part 2: Successful edit ---
        write_draft(POST_HISTORY_DRAFT_PATH, "Write in third person always.")
            .await
            .unwrap();

        let edit_tool = EditPostHistoryDraftTool::new(tx.clone());
        let edit_args = serde_json::json!({"old_text": "third person", "new_text": "first person"});
        let result = edit_tool.execute(edit_args).await.unwrap();

        assert_eq!(result["success"], true);
        assert_eq!(result["path"], POST_HISTORY_DRAFT_PATH);

        // Verify Preview event was sent with updated content
        let event = rx.recv().await.unwrap();
        match event {
            WsEvent::Preview { tab, data } => {
                assert_eq!(tab, "posthistory");
                assert_eq!(
                    data,
                    Value::String("Write in first person always.".to_string())
                );
            }
            _ => panic!("Expected Preview event"),
        }

        // --- Part 3: Text not found error ---
        let edit_tool = EditPostHistoryDraftTool::new(tx);
        let args = serde_json::json!({"old_text": "unicorns", "new_text": "griffins"});
        let result = edit_tool.execute(args).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Text not found in draft"),
            "Expected 'Text not found' error, got: {}",
            err_msg
        );
    }
}
