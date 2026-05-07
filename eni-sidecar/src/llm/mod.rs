//! LLM client module — OpenAI-compatible chat completion with SSE streaming.
//!
//! Provides `LlmClient` for making streaming chat completion requests to any
//! OpenAI-compatible API endpoint. Handles SSE parsing, tool call accumulation,
//! and model profile switching at runtime.

mod client;
mod types;

pub use client::LlmClient;
pub use types::*;
