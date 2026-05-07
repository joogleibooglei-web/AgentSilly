//! Context builder module — assembles LLM prompt from system prompt, history, and tool definitions.
//!
//! The context builder handles:
//! - Assembling the system message (ENI personality + post-card + reference chunks)
//! - Including conversation history
//! - Token counting with tiktoken-rs for budget enforcement
//! - Truncation: removing oldest messages when over budget while preserving system prompt + last 4 messages

pub mod builder;

pub use builder::{ContextBuilder, DocumentChunk, count_message_tokens, count_tokens};
