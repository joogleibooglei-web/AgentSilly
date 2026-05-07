//! Tool implementations module — tool trait, dispatcher, and individual tools.
//!
//! The dispatcher routes tool calls to registered implementations, validates
//! arguments against JSON schemas, and returns structured results.

pub mod dispatcher;

pub use dispatcher::{Tool, ToolDispatcher, ToolResult, validate_against_schema};
