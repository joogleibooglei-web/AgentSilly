//! Agent loop module — core execution cycle.
//!
//! Handles: user message → context assembly → LLM call → tool execution → stream response.
//!
//! The agent loop follows the Codex pattern:
//! 1. User sends a message
//! 2. Context builder assembles the prompt (system + history + tools)
//! 3. LLM is called with streaming
//! 4. If the LLM returns text → stream tokens to frontend, done
//! 5. If the LLM returns tool calls → execute tools, append results, loop back to step 2
//! 6. Iteration limit prevents infinite loops (default: 15)

pub mod events;
pub mod r#loop;
pub mod session;

pub use events::{AgentState, WsEvent};
pub use r#loop::{run_turn, AgentContext};
pub use session::SharedSessionContext;
