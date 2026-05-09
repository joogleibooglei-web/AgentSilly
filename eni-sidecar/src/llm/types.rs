//! Types for the LLM client — request/response models for OpenAI-compatible APIs.

use serde::{Deserialize, Serialize};

/// A chat message in the OpenAI chat-completion format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<String>,
    /// Tool calls made by the assistant (only present in assistant messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
    /// Tool call ID this message is responding to (only present in tool messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Optional name field (used for tool results).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn assistant_tool_calls(tool_calls: Vec<ChatToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
            name: None,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            name: None,
        }
    }
}

/// A tool call as it appears in an assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ChatFunctionCall,
}

/// The function portion of a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatFunctionCall {
    pub name: String,
    pub arguments: String,
}

/// A tool definition in the OpenAI function-calling format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

/// Function schema within a tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl ToolDefinition {
    /// Create a new tool definition with the given name, description, and JSON schema parameters.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

/// The result of a streaming chat completion — either text content or one or more tool calls.
#[derive(Debug, Clone)]
pub enum LlmResponse {
    /// The LLM produced a text response (final answer).
    Text(String),
    /// The LLM requested one or more tool calls.
    ToolCalls(Vec<ToolCall>),
}

/// A parsed tool call extracted from the LLM response.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// The unique ID assigned by the API for this tool call.
    pub id: String,
    /// The tool/function name to invoke.
    pub name: String,
    /// The parsed arguments as a JSON value.
    pub arguments: serde_json::Value,
    /// The raw arguments string (for inclusion in conversation history).
    pub arguments_raw: String,
}

/// A callback for receiving streamed tokens as they arrive.
/// The client calls this with each text delta so the caller can relay to the frontend.
pub type TokenCallback = Box<dyn Fn(&str) + Send + Sync>;

/// A callback for receiving streamed thinking/reasoning tokens.
/// The client calls this with each reasoning delta so the caller can relay to the frontend.
pub type ThinkingCallback = Box<dyn Fn(&str) + Send + Sync>;

// --- SSE response types (internal, for parsing the API response) ---

/// A single SSE chunk from the streaming chat completion response.
#[derive(Debug, Deserialize)]
pub(crate) struct StreamChunk {
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StreamChoice {
    pub delta: StreamDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StreamDelta {
    pub content: Option<String>,
    /// Reasoning/thinking content (used by DeepSeek, OpenRouter, etc.)
    pub reasoning_content: Option<String>,
    pub tool_calls: Option<Vec<StreamToolCallDelta>>,
}

/// A partial tool call delta in the SSE stream.
/// The API sends these incrementally — the index identifies which tool call
/// is being built, and the function name/arguments arrive in pieces.
#[derive(Debug, Deserialize)]
pub(crate) struct StreamToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: Option<StreamFunctionDelta>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StreamFunctionDelta {
    pub name: Option<String>,
    pub arguments: Option<String>,
}
