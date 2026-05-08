//! WebSocket protocol types — client messages and sender wrapper.

use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::warn;

use crate::agent::WsEvent;

/// Messages sent from the frontend client to the sidecar via WebSocket.
///
/// Deserialized from JSON using serde's internally-tagged enum representation.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// User sends a chat message to the agent.
    UserMessage {
        content: String,
    },
    /// User requests cancellation of the active agent turn.
    Cancel,
    /// User requests switching to a different model profile.
    SwitchModel {
        profile: String,
    },
    /// User starts a new conversation (archives the current one).
    NewConversation,
    /// User requests an undo operation on a specific entity.
    Undo {
        entity_type: String,
        entity_id: String,
    },
    /// User updates a configuration value.
    UpdateConfig {
        key: String,
        value: serde_json::Value,
    },
    /// User requests a test of the LLM connection with current settings.
    TestConnection,
}

/// A sender wrapper that serializes `WsEvent` variants and relays them
/// to the WebSocket write task via an internal mpsc channel.
///
/// This decouples the agent loop from WebSocket write backpressure —
/// the agent loop sends events into the channel, and a background task
/// drains the channel and writes to the actual WebSocket sink.
#[derive(Clone)]
pub struct WebSocketSender {
    tx: mpsc::Sender<WsEvent>,
}

impl WebSocketSender {
    /// Create a new `WebSocketSender` wrapping the given channel sender.
    pub fn new(tx: mpsc::Sender<WsEvent>) -> Self {
        Self { tx }
    }

    /// Send a `WsEvent` to the connected client.
    ///
    /// Returns `Ok(())` if the event was queued, or `Err(())` if the
    /// channel is closed (client disconnected).
    pub async fn send(&self, event: WsEvent) -> Result<(), ()> {
        self.tx.send(event).await.map_err(|_| {
            warn!("WebSocket sender channel closed — client likely disconnected");
        })
    }

    /// Get a reference to the underlying mpsc sender.
    ///
    /// This is used to pass directly to the agent loop which expects
    /// `&mpsc::Sender<WsEvent>`.
    pub fn inner(&self) -> &mpsc::Sender<WsEvent> {
        &self.tx
    }
}
