//! Tool dispatcher — routes tool calls to registered implementations and validates arguments.

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, warn};

use crate::llm::{ToolCall, ToolDefinition};

/// The result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the tool executed successfully.
    pub success: bool,
    /// The result data (on success) or error message (on failure).
    pub data: serde_json::Value,
}

impl ToolResult {
    /// Create a successful tool result.
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data,
        }
    }

    /// Create an error tool result.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: serde_json::Value::String(message.into()),
        }
    }

    /// Get the result as a string for inclusion in conversation history.
    pub fn to_content_string(&self) -> String {
        match &self.data {
            serde_json::Value::String(s) => s.clone(),
            other => serde_json::to_string_pretty(other).unwrap_or_default(),
        }
    }
}

/// Trait that all tool implementations must satisfy.
///
/// Each tool provides its name, description, parameter schema, argument validation,
/// and async execution logic.
#[async_trait]
pub trait Tool: Send + Sync {
    /// The unique name of this tool (used for routing).
    fn name(&self) -> &str;

    /// A human-readable description of what this tool does.
    fn description(&self) -> &str;

    /// The JSON Schema describing the tool's parameters.
    /// This is included in the OpenAI function-calling `tools` array.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Validate the provided arguments against this tool's schema.
    /// Returns Ok(()) if valid, or an error describing what's wrong.
    fn validate_args(&self, args: &serde_json::Value) -> Result<()>;

    /// Execute the tool with the given arguments.
    /// Returns the result data on success, or an error.
    async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value>;
}

/// Routes tool calls to registered tool implementations.
///
/// Maintains a registry of tools by name and handles:
/// - Looking up tools by name
/// - Validating arguments before execution
/// - Executing tools and wrapping results in `ToolResult`
pub struct ToolDispatcher {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolDispatcher {
    /// Create a new empty tool dispatcher.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool with the dispatcher.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        debug!(tool = %name, "Registering tool");
        self.tools.insert(name, tool);
    }

    /// Get the list of tool definitions for the LLM (OpenAI function-calling format).
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tool| {
                ToolDefinition::new(
                    tool.name(),
                    tool.description(),
                    tool.parameters_schema(),
                )
            })
            .collect()
    }

    /// Execute a tool call.
    ///
    /// Looks up the tool by name, validates arguments, and executes.
    /// Returns a `ToolResult` with success/error status.
    pub async fn execute(&self, call: &ToolCall) -> ToolResult {
        let Some(tool) = self.tools.get(&call.name) else {
            warn!(tool = %call.name, "Unknown tool requested");
            return ToolResult::error(format!("Unknown tool: {}", call.name));
        };

        // Validate arguments against schema
        if let Err(e) = tool.validate_args(&call.arguments) {
            warn!(
                tool = %call.name,
                error = %e,
                "Tool argument validation failed"
            );
            return ToolResult::error(format!("Invalid arguments: {}", e));
        }

        // Execute the tool
        debug!(tool = %call.name, "Executing tool");
        match tool.execute(call.arguments.clone()).await {
            Ok(data) => {
                debug!(tool = %call.name, "Tool executed successfully");
                ToolResult::success(data)
            }
            Err(e) => {
                error!(tool = %call.name, error = %e, "Tool execution failed");
                ToolResult::error(e.to_string())
            }
        }
    }

    /// Check if a tool with the given name is registered.
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get the number of registered tools.
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }
}

impl Default for ToolDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to validate arguments against a JSON schema.
///
/// Uses the jsonschema crate for validation. Returns Ok(()) if valid,
/// or an error with a description of the first validation failure.
pub fn validate_against_schema(
    schema: &serde_json::Value,
    instance: &serde_json::Value,
) -> Result<()> {
    let compiled = jsonschema::JSONSchema::compile(schema)
        .map_err(|e| anyhow::anyhow!("Invalid JSON schema: {}", e))?;

    let result = compiled.validate(instance);
    if let Err(errors) = result {
        let error_messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        if !error_messages.is_empty() {
            anyhow::bail!("{}", error_messages.join("; "));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple test tool for unit testing the dispatcher.
    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echoes back the input message"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "The message to echo"
                    }
                },
                "required": ["message"]
            })
        }

        fn validate_args(&self, args: &serde_json::Value) -> Result<()> {
            validate_against_schema(&self.parameters_schema(), args)
        }

        async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value> {
            let message = args["message"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing message field"))?;
            Ok(serde_json::json!({ "echoed": message }))
        }
    }

    /// A tool that always fails for testing error handling.
    struct FailingTool;

    #[async_trait]
    impl Tool for FailingTool {
        fn name(&self) -> &str {
            "fail"
        }

        fn description(&self) -> &str {
            "Always fails"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        }

        fn validate_args(&self, _args: &serde_json::Value) -> Result<()> {
            Ok(())
        }

        async fn execute(&self, _args: serde_json::Value) -> Result<serde_json::Value> {
            anyhow::bail!("Tool execution intentionally failed")
        }
    }

    #[tokio::test]
    async fn test_execute_registered_tool() {
        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(Box::new(EchoTool));

        let call = ToolCall {
            id: "call_1".to_string(),
            name: "echo".to_string(),
            arguments: serde_json::json!({"message": "hello"}),
            arguments_raw: r#"{"message":"hello"}"#.to_string(),
        };

        let result = dispatcher.execute(&call).await;
        assert!(result.success);
        assert_eq!(result.data["echoed"], "hello");
    }

    #[tokio::test]
    async fn test_unknown_tool_returns_error() {
        let dispatcher = ToolDispatcher::new();

        let call = ToolCall {
            id: "call_1".to_string(),
            name: "nonexistent".to_string(),
            arguments: serde_json::json!({}),
            arguments_raw: "{}".to_string(),
        };

        let result = dispatcher.execute(&call).await;
        assert!(!result.success);
        assert!(result.to_content_string().contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_invalid_arguments_returns_error() {
        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(Box::new(EchoTool));

        // Missing required "message" field
        let call = ToolCall {
            id: "call_1".to_string(),
            name: "echo".to_string(),
            arguments: serde_json::json!({}),
            arguments_raw: "{}".to_string(),
        };

        let result = dispatcher.execute(&call).await;
        assert!(!result.success);
        assert!(result.to_content_string().contains("Invalid arguments"));
    }

    #[tokio::test]
    async fn test_tool_execution_failure() {
        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(Box::new(FailingTool));

        let call = ToolCall {
            id: "call_1".to_string(),
            name: "fail".to_string(),
            arguments: serde_json::json!({}),
            arguments_raw: "{}".to_string(),
        };

        let result = dispatcher.execute(&call).await;
        assert!(!result.success);
        assert!(result.to_content_string().contains("intentionally failed"));
    }

    #[tokio::test]
    async fn test_tool_definitions() {
        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(Box::new(EchoTool));
        dispatcher.register(Box::new(FailingTool));

        let defs = dispatcher.tool_definitions();
        assert_eq!(defs.len(), 2);

        // Check that definitions have the right format
        let echo_def = defs.iter().find(|d| d.function.name == "echo").unwrap();
        assert_eq!(echo_def.tool_type, "function");
        assert_eq!(echo_def.function.description, "Echoes back the input message");
    }

    #[tokio::test]
    async fn test_has_tool() {
        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(Box::new(EchoTool));

        assert!(dispatcher.has_tool("echo"));
        assert!(!dispatcher.has_tool("nonexistent"));
    }

    #[test]
    fn test_tool_result_to_content_string() {
        let success = ToolResult::success(serde_json::json!({"key": "value"}));
        let content = success.to_content_string();
        assert!(content.contains("key"));
        assert!(content.contains("value"));

        let error = ToolResult::error("Something went wrong");
        assert_eq!(error.to_content_string(), "Something went wrong");
    }

    #[test]
    fn test_validate_against_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" }
            },
            "required": ["name"]
        });

        // Valid
        let valid = serde_json::json!({"name": "Kael", "age": 30});
        assert!(validate_against_schema(&schema, &valid).is_ok());

        // Missing required field
        let invalid = serde_json::json!({"age": 30});
        assert!(validate_against_schema(&schema, &invalid).is_err());

        // Wrong type
        let wrong_type = serde_json::json!({"name": 123});
        assert!(validate_against_schema(&schema, &wrong_type).is_err());
    }
}
