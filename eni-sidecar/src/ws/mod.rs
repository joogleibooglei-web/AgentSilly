//! WebSocket server module — handles client connections and message relay.

pub mod server;
pub mod types;

pub use server::{start, AppState};
pub use types::{ClientMessage, WebSocketSender};
