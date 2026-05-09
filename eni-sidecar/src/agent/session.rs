use std::sync::Arc;
use tokio::sync::Mutex;

/// Tracks session-level state across tool invocations within a single connection.
#[derive(Debug, Clone, Default)]
pub struct SessionContext {
    /// The avatar_url of the last character the agent read or wrote.
    /// Used by finalize tools to determine the target character.
    pub last_avatar_url: Option<String>,
}

pub type SharedSessionContext = Arc<Mutex<SessionContext>>;
