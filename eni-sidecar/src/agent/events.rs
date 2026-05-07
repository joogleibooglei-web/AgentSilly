//! WebSocket event types sent from the sidecar to the frontend.
//!
//! These events are serialized as JSON and sent over the WebSocket connection
//! to update the Svelte frontend in real-time.

use serde::Serialize;

/// Events sent from the sidecar to the frontend via WebSocket.
///
/// Serializes to JSON matching the protocol defined in the design document.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    /// A streamed text token from the LLM response.
    Token {
        content: String,
    },

    /// Thinking/reasoning content (for collapsible block in UI).
    Thinking {
        content: String,
    },

    /// The assistant's message is complete.
    MessageComplete {
        id: String,
    },

    /// A tool execution is starting.
    ToolStart {
        name: String,
        description: String,
    },

    /// A tool execution has completed.
    ToolEnd {
        name: String,
        success: bool,
    },

    /// Preview data for the right pane.
    Preview {
        tab: String,
        data: serde_json::Value,
    },

    /// An error occurred.
    Error {
        message: String,
    },

    /// Agent state change.
    Status {
        state: AgentState,
    },

    /// An undo operation is available after a write.
    UndoAvailable {
        entity_type: String,
        entity_id: String,
        summary: String,
    },

    /// A system message (e.g., "Generation stopped").
    SystemMessage {
        content: String,
    },

    /// Configuration was updated.
    ConfigUpdated {
        key: String,
    },
}

/// The current state of the agent.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Idle,
    Thinking,
    ToolExecuting,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_event_serialization() {
        let event = WsEvent::Token {
            content: "Hello".to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "token");
        assert_eq!(json["content"], "Hello");
    }

    #[test]
    fn test_tool_start_event_serialization() {
        let event = WsEvent::ToolStart {
            name: "read_character".to_string(),
            description: "Reading character data".to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "tool_start");
        assert_eq!(json["name"], "read_character");
        assert_eq!(json["description"], "Reading character data");
    }

    #[test]
    fn test_status_event_serialization() {
        let event = WsEvent::Status {
            state: AgentState::Thinking,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "status");
        assert_eq!(json["state"], "thinking");
    }

    #[test]
    fn test_error_event_serialization() {
        let event = WsEvent::Error {
            message: "Something went wrong".to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "error");
        assert_eq!(json["message"], "Something went wrong");
    }

    #[test]
    fn test_message_complete_serialization() {
        let event = WsEvent::MessageComplete {
            id: "msg-123".to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "message_complete");
        assert_eq!(json["id"], "msg-123");
    }
}
