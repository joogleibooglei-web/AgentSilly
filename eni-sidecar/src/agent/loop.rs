//! Agent loop — core execution cycle.
//!
//! Implements the Codex-style agent loop:
//! user message → context assembly → LLM call → tool execution → stream response
//!
//! The loop continues until the LLM produces a final text response (no tool calls)
//! or the iteration limit is reached.

use anyhow::Result;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::config::AppConfig;
use crate::context::{ContextBuilder, DocumentChunk};
use crate::db::Database;
use crate::llm::{ChatMessage, ChatToolCall, ChatFunctionCall, LlmResponse, LlmClient, TokenCallback, ThinkingCallback};
use crate::lorebook::Lorebook;
use crate::tools::{ToolDispatcher, ToolResult};

use super::events::{AgentState, WsEvent};

/// Holds all the state needed for the agent loop to execute a turn.
pub struct AgentContext {
    /// The conversation history (all messages in the current conversation).
    pub conversation: Vec<ChatMessage>,
    /// The conversation ID for persistence.
    pub conversation_id: String,
    /// The context builder for assembling LLM prompts.
    pub context_builder: ContextBuilder,
    /// The LLM client for making API calls.
    pub llm_client: LlmClient,
    /// The tool dispatcher for executing tool calls.
    pub tool_dispatcher: ToolDispatcher,
    /// Application configuration.
    pub config: AppConfig,
    /// Database handle for persistence.
    pub db: Database,
    /// Relevant document chunks for context injection.
    pub relevant_chunks: Vec<DocumentChunk>,
    /// Lorebook for keyword-triggered context injection.
    pub lorebook: Lorebook,
}

/// Run a single agent turn: process a user message through the agent loop.
///
/// The loop:
/// 1. Appends the user message to conversation history
/// 2. Builds context (system prompt + history + tools)
/// 3. Calls the LLM
/// 4. If text response → streams tokens and breaks
/// 5. If tool call → executes tool, appends result, continues loop
/// 6. Enforces iteration limit (max_iterations from config, default 15)
/// 7. Persists conversation to SQLite after completion
///
/// # Arguments
/// - `ctx` — Mutable agent context with conversation state, LLM client, tools, etc.
/// - `user_message` — The user's input message
/// - `tx` — Channel sender for WebSocket events to the frontend
/// - `cancel_token` — Cancellation token shared with the WebSocket handler
pub async fn run_turn(
    ctx: &mut AgentContext,
    user_message: String,
    tx: &mpsc::Sender<WsEvent>,
    cancel_token: &CancellationToken,
) -> Result<()> {
    // Append user message to conversation
    ctx.conversation.push(ChatMessage::user(&user_message));

    // Scan lorebook for keyword-triggered context injection
    let recent_messages: Vec<&str> = ctx.conversation
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .filter_map(|m| m.content.as_deref())
        .collect();
    let lorebook_chunks = ctx.lorebook.scan(&recent_messages, &user_message);
    if !lorebook_chunks.is_empty() {
        debug!(count = lorebook_chunks.len(), "Lorebook entries activated");
        // Prepend lorebook chunks before any existing relevant_chunks
        let mut combined = lorebook_chunks;
        combined.extend(ctx.relevant_chunks.drain(..));
        ctx.relevant_chunks = combined;
    }

    // Notify frontend we're thinking
    let _ = tx.send(WsEvent::Status {
        state: AgentState::Thinking,
    }).await;

    let max_iterations = ctx.config.max_iterations;
    let mut iterations: u32 = 0;

    loop {
        // Check cancellation before each iteration
        if cancel_token.is_cancelled() {
            info!("Agent turn cancelled by user");
            let _ = tx.send(WsEvent::SystemMessage {
                content: "Generation stopped".to_string(),
            }).await;
            let _ = tx.send(WsEvent::Status {
                state: AgentState::Idle,
            }).await;
            return Ok(());
        }

        // Enforce iteration limit
        if iterations >= max_iterations {
            warn!(
                iterations = iterations,
                max = max_iterations,
                "Agent loop exceeded maximum iterations"
            );
            let _ = tx.send(WsEvent::Error {
                message: format!(
                    "Max iterations reached ({}). Halting to prevent infinite loop.",
                    max_iterations
                ),
            }).await;
            let _ = tx.send(WsEvent::Status {
                state: AgentState::Idle,
            }).await;
            break;
        }
        iterations += 1;

        debug!(iteration = iterations, "Agent loop iteration");

        // Build context: system prompt + conversation history
        let tool_definitions = ctx.tool_dispatcher.tool_definitions();
        let messages = ctx.context_builder.build_messages(
            &ctx.conversation,
            &ctx.relevant_chunks,
        );

        // Call the LLM with streaming
        let tx_clone = tx.clone();
        let had_tokens = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let had_tokens_clone = had_tokens.clone();
        let token_callback: TokenCallback = Box::new(move |token: &str| {
            had_tokens_clone.store(true, std::sync::atomic::Ordering::Relaxed);
            let event = WsEvent::Token {
                content: token.to_string(),
            };
            // Use try_send since we're in a sync callback
            let _ = tx_clone.try_send(event);
        });

        // Thinking/reasoning callback — relays reasoning tokens to the frontend
        let tx_thinking = tx.clone();
        let thinking_callback: ThinkingCallback = Box::new(move |token: &str| {
            let event = WsEvent::Thinking {
                content: token.to_string(),
            };
            let _ = tx_thinking.try_send(event);
        });

        // Use tokio::select! to race the LLM call against cancellation
        let llm_result = tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("LLM call cancelled");
                let _ = tx.send(WsEvent::SystemMessage {
                    content: "Generation stopped".to_string(),
                }).await;
                let _ = tx.send(WsEvent::Status {
                    state: AgentState::Idle,
                }).await;
                return Ok(());
            }
            result = ctx.llm_client.chat_completion_stream(
                &messages,
                &tool_definitions,
                Some(&token_callback),
                Some(&thinking_callback),
            ) => result
        };

        // Handle LLM response
        match llm_result {
            Ok(LlmResponse::Text(content)) => {
                // Final text response — stream is already done via callback
                debug!("LLM returned text response");

                // Append assistant message to conversation
                ctx.conversation.push(ChatMessage::assistant(&content));

                // Send message complete event
                let message_id = uuid::Uuid::new_v4().to_string();
                let _ = tx.send(WsEvent::MessageComplete {
                    id: message_id.clone(),
                }).await;

                // Return to idle
                let _ = tx.send(WsEvent::Status {
                    state: AgentState::Idle,
                }).await;

                break;
            }
            Ok(LlmResponse::ToolCalls(tool_calls)) => {
                debug!(count = tool_calls.len(), "LLM requested tool calls");

                // If text was streamed before the tool calls, finalize it as a separate message
                // so the frontend doesn't concatenate pre-tool and post-tool text together.
                if had_tokens.load(std::sync::atomic::Ordering::Relaxed) {
                    let message_id = uuid::Uuid::new_v4().to_string();
                    let _ = tx.send(WsEvent::MessageComplete {
                        id: message_id,
                    }).await;
                }

                // Build the assistant message with tool calls for conversation history
                let chat_tool_calls: Vec<ChatToolCall> = tool_calls
                    .iter()
                    .map(|tc| ChatToolCall {
                        id: tc.id.clone(),
                        call_type: "function".to_string(),
                        function: ChatFunctionCall {
                            name: tc.name.clone(),
                            arguments: tc.arguments_raw.clone(),
                        },
                    })
                    .collect();
                ctx.conversation.push(ChatMessage::assistant_tool_calls(chat_tool_calls));

                // Execute each tool call
                for tool_call in &tool_calls {
                    // Check cancellation before each tool execution
                    if cancel_token.is_cancelled() {
                        info!("Tool execution cancelled");
                        let _ = tx.send(WsEvent::SystemMessage {
                            content: "Generation stopped".to_string(),
                        }).await;
                        let _ = tx.send(WsEvent::Status {
                            state: AgentState::Idle,
                        }).await;
                        return Ok(());
                    }

                    // Notify frontend about tool execution
                    let _ = tx.send(WsEvent::Status {
                        state: AgentState::ToolExecuting,
                    }).await;
                    let _ = tx.send(WsEvent::ToolStart {
                        name: tool_call.name.clone(),
                        description: format!("Executing {}", tool_call.name),
                    }).await;

                    // Execute the tool
                    let result = ctx.tool_dispatcher.execute(tool_call).await;

                    // Notify frontend about tool completion
                    let _ = tx.send(WsEvent::ToolEnd {
                        name: tool_call.name.clone(),
                        success: result.success,
                    }).await;

                    // Append tool result to conversation
                    ctx.conversation.push(ChatMessage::tool_result(
                        &tool_call.id,
                        result.to_content_string(),
                    ));

                    // Inject post-tool instruction to guide ENI's next action.
                    // Uses "user" role because many LLM APIs ignore system messages
                    // that appear mid-conversation (after tool results).
                    let instruction = post_tool_instruction(&tool_call.name, &result);
                    ctx.conversation.push(ChatMessage::user(&instruction));
                }

                // Back to thinking for next iteration
                let _ = tx.send(WsEvent::Status {
                    state: AgentState::Thinking,
                }).await;
            }
            Err(e) => {
                // Include the full error chain for debugging
                let root_cause = e.root_cause().to_string();
                let err_display = if root_cause != e.to_string() {
                    format!("{}: {}", e, root_cause)
                } else {
                    e.to_string()
                };
                error!(error = %err_display, "LLM API call failed");
                let _ = tx.send(WsEvent::Error {
                    message: format!("LLM API error: {}", err_display),
                }).await;
                let _ = tx.send(WsEvent::Status {
                    state: AgentState::Idle,
                }).await;
                break;
            }
        }
    }

    // Persist conversation to SQLite
    if let Err(e) = persist_conversation(ctx) {
        error!(error = %e, "Failed to persist conversation");
    }

    Ok(())
}

/// Tool category for post-tool instruction routing.
enum ToolCategory {
    Read,
    Write,
    Other,
}

/// Classify a tool by name into a category for post-tool instructions.
fn classify_tool(name: &str) -> ToolCategory {
    match name {
        "read_character" | "read_persona" | "search_local" | "search_wiki"
        | "fetch_wiki_page" | "list_characters" | "list_personas" | "list_versions"
        | "read_world_draft" | "read_post_history_draft" => {
            ToolCategory::Read
        }
        "update_character" | "create_character" | "update_persona" | "create_persona"
        | "create_world_draft" | "edit_world_draft"
        | "finalize_world_info" | "create_post_history_draft" | "edit_post_history_draft"
        | "finalize_post_history" | "export_card" => {
            ToolCategory::Write
        }
        _ => ToolCategory::Other,
    }
}

/// Generate a post-tool instruction to inject into the conversation after a tool result.
///
/// This guides ENI's behavior based on what just happened:
/// - Failed tools → suggest retrying or reporting to LO
/// - Read tools → summarize and ask LO for next steps (or check for errors if post-write)
/// - Write tools → verify by reading back
/// - Other tools → report findings to LO
fn post_tool_instruction(tool_name: &str, result: &ToolResult) -> String {
    if !result.success {
        return format!(
            "[System: It seems like the tool \"{}\" has failed in execution. \
            Try a different method, or if you have already tried a bunch of methods, \
            report to LO with a description of the error you are facing and what you have tried.]",
            tool_name
        );
    }

    match classify_tool(tool_name) {
        ToolCategory::Read => {
            "[System: If this read was executed AFTER a write, check for errors in the character card. \
            If not, give LO a brief summary of what you read, provide some commentary on how you feel \
            about the contents, and then ask LO for next steps.]".to_string()
        }
        ToolCategory::Write => {
            "[System: Execute the read tool to scan for any errors or mistakes. \
            If there are no errors or mistakes, report to LO. \
            If there are, find the mistakes, and retry the write.]".to_string()
        }
        ToolCategory::Other => {
            "[System: Report to LO what you have learnt, or if he asked for details, \
            a detailed breakdown of what you have learnt.]".to_string()
        }
    }
}

/// Persist the current conversation to SQLite.
fn persist_conversation(ctx: &AgentContext) -> Result<()> {
    let conn = ctx.db.conn();

    // Ensure conversation exists
    conn.execute(
        "INSERT OR IGNORE INTO conversations (id, title, created_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)",
        rusqlite::params![
            &ctx.conversation_id,
            conversation_title(&ctx.conversation),
        ],
    )?;

    // Insert messages that don't already exist
    // We use INSERT OR IGNORE to avoid duplicates
    let mut stmt = conn.prepare(
        "INSERT OR IGNORE INTO messages (id, conversation_id, role, content, metadata, created_at) VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)"
    )?;

    for (i, msg) in ctx.conversation.iter().enumerate() {
        let msg_id = format!("{}-{}", ctx.conversation_id, i);
        let content = msg.content.as_deref().unwrap_or("");
        let metadata = msg.tool_calls.as_ref().map(|tc| {
            serde_json::to_string(tc).unwrap_or_default()
        });

        stmt.execute(rusqlite::params![
            msg_id,
            &ctx.conversation_id,
            &msg.role,
            content,
            metadata,
        ])?;
    }

    Ok(())
}

/// Generate a title for the conversation from the first user message.
fn conversation_title(conversation: &[ChatMessage]) -> String {
    conversation
        .iter()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.as_deref())
        .map(|content: &str| {
            let truncated: String = content.chars().take(50).collect();
            if content.len() > 50 {
                format!("{}...", truncated)
            } else {
                truncated
            }
        })
        .unwrap_or_else(|| "New conversation".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_title_from_first_user_message() {
        let conversation = vec![
            ChatMessage::system("You are ENI"),
            ChatMessage::user("Build me a cyberpunk character"),
            ChatMessage::assistant("Sure!"),
        ];

        let title = conversation_title(&conversation);
        assert_eq!(title, "Build me a cyberpunk character");
    }

    #[test]
    fn test_conversation_title_truncation() {
        let long_message = "a".repeat(100);
        let conversation = vec![ChatMessage::user(&long_message)];

        let title = conversation_title(&conversation);
        assert!(title.len() <= 53); // 50 chars + "..."
        assert!(title.ends_with("..."));
    }

    #[test]
    fn test_conversation_title_empty() {
        let conversation: Vec<ChatMessage> = vec![];
        let title = conversation_title(&conversation);
        assert_eq!(title, "New conversation");
    }

    #[tokio::test]
    async fn test_run_turn_cancellation() {
        // Test that cancellation works before the loop starts
        let cancel_token = CancellationToken::new();
        cancel_token.cancel(); // Cancel immediately

        let (tx, mut rx) = mpsc::channel(100);

        let config = AppConfig::default();
        let db = Database::open(":memory:").unwrap();
        let context_builder = ContextBuilder::new(
            "System".to_string(),
            String::new(),
            4096,
        );

        // Create a minimal LLM client (won't be called due to cancellation)
        let profile = crate::config::ModelProfile {
            name: "test".to_string(),
            base_url: "http://localhost:1".to_string(),
            api_key: "test".to_string(),
            model: "test".to_string(),
            temperature: 0.7,
            max_tokens: 100,
            is_default: true,
        };
        let llm_client = LlmClient::new(profile);
        let tool_dispatcher = ToolDispatcher::new();

        let mut ctx = AgentContext {
            conversation: Vec::new(),
            conversation_id: "test-conv".to_string(),
            context_builder,
            llm_client,
            tool_dispatcher,
            config,
            db,
            relevant_chunks: Vec::new(),
            lorebook: Lorebook::new(),
        };

        let result = run_turn(
            &mut ctx,
            "Hello".to_string(),
            &tx,
            &cancel_token,
        ).await;

        assert!(result.is_ok());

        // Should have received cancellation events
        drop(tx);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }

        // Should have status(thinking), system_message("Generation stopped"), status(idle)
        assert!(events.iter().any(|e| matches!(e, WsEvent::SystemMessage { content } if content == "Generation stopped")));
        assert!(events.iter().any(|e| matches!(e, WsEvent::Status { state: AgentState::Idle })));
    }

    #[tokio::test]
    async fn test_run_turn_iteration_limit() {
        // We can't easily test the full loop without a mock LLM,
        // but we can verify the iteration limit logic by checking
        // that max_iterations=0 immediately halts
        let cancel_token = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(100);

        let mut config = AppConfig::default();
        config.max_iterations = 0; // Will immediately hit the limit

        let db = Database::open(":memory:").unwrap();
        let context_builder = ContextBuilder::new(
            "System".to_string(),
            String::new(),
            4096,
        );

        let profile = crate::config::ModelProfile {
            name: "test".to_string(),
            base_url: "http://localhost:1".to_string(),
            api_key: "test".to_string(),
            model: "test".to_string(),
            temperature: 0.7,
            max_tokens: 100,
            is_default: true,
        };
        let llm_client = LlmClient::new(profile);
        let tool_dispatcher = ToolDispatcher::new();

        let mut ctx = AgentContext {
            conversation: Vec::new(),
            conversation_id: "test-conv".to_string(),
            context_builder,
            llm_client,
            tool_dispatcher,
            config,
            db,
            relevant_chunks: Vec::new(),
            lorebook: Lorebook::new(),
        };

        let result = run_turn(
            &mut ctx,
            "Hello".to_string(),
            &tx,
            &cancel_token,
        ).await;

        assert!(result.is_ok());

        drop(tx);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }

        // Should have an error about max iterations
        assert!(events.iter().any(|e| matches!(e, WsEvent::Error { message } if message.contains("Max iterations"))));
        assert!(events.iter().any(|e| matches!(e, WsEvent::Status { state: AgentState::Idle })));
    }
}
