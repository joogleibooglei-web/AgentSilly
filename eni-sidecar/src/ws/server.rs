//! WebSocket server — accepts connections, routes messages, relays events.
//!
//! Uses `tokio-tungstenite` directly with `tokio::net::TcpListener`.
//! Each connection gets its own handler task with a dedicated `CancellationToken`
//! and `WebSocketSender`.

use std::sync::Arc;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::agent::events::{AgentState, WsEvent};
use crate::agent::{self, AgentContext};
use crate::config::AppConfig;
use crate::context::ContextBuilder;
use crate::db::Database;
use crate::llm::LlmClient;
use crate::prompts::ENI_SYSTEM_PROMPT;
use crate::search::SearchIndex;
use crate::tools::{
    self, ToolDispatcher, ExportCardTool, ListCharactersTool, ReadCharacterTool,
    WriteCharacterTool, ReadPersonaTool, WritePersonaTool, ListPersonasTool,
    ReadPostHistoryTool, WritePostHistoryTool, ReadWorldEntriesTool, WriteWorldEntryTool,
    SearchLocalTool, SearchWikiTool, FetchWikiPageTool, ShowPreviewTool, CreateProjectTool, ManageTasksTool,
    UndoChangeTool, ListVersionsTool, StClient,
};
use crate::versioning::VersionStore;

use super::types::{ClientMessage, WebSocketSender};

/// Shared application state accessible by all connection handlers.
pub struct AppState {
    pub config: AppConfig,
    pub db: Arc<std::sync::Mutex<Database>>,
    pub version_store: Arc<VersionStore>,
    pub llm_client: LlmClient,
    pub tool_dispatcher: Arc<ToolDispatcher>,
    pub search_index: Arc<SearchIndex>,
}

/// Start the WebSocket server on the configured port.
///
/// Listens for incoming TCP connections and upgrades them to WebSocket.
/// Each connection is handled in its own spawned task.
pub async fn start(state: Arc<AppState>) -> Result<()> {
    let addr = format!("0.0.0.0:{}", state.config.listen_port);
    let listener = TcpListener::bind(&addr).await?;

    info!(address = %addr, "WebSocket server listening");

    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                error!(error = %e, "Failed to accept TCP connection");
                continue;
            }
        };

        info!(peer = %peer_addr, "New TCP connection");

        let state = Arc::clone(&state);
        tokio::spawn(async move {
            match tokio_tungstenite::accept_async(stream).await {
                Ok(ws_stream) => {
                    info!(peer = %peer_addr, "WebSocket connection established");
                    if let Err(e) = handle_connection(ws_stream, state).await {
                        error!(peer = %peer_addr, error = %e, "Connection handler error");
                    }
                    info!(peer = %peer_addr, "WebSocket connection closed");
                }
                Err(e) => {
                    error!(peer = %peer_addr, error = %e, "WebSocket handshake failed");
                }
            }
        });
    }
}

/// Handle a single WebSocket connection.
///
/// Sets up the event relay channel, spawns the write task, and processes
/// incoming messages in a loop. Only one agent turn runs at a time.
async fn handle_connection(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    state: Arc<AppState>,
) -> Result<()> {
    let (ws_sink, mut ws_source) = ws_stream.split();

    // Channel for events from the agent loop to the WS write task.
    // Buffer of 256 should handle bursts of streaming tokens.
    let (event_tx, mut event_rx) = mpsc::channel::<WsEvent>(256);

    // Wrap the sink in a mutex so the write task owns it exclusively.
    let ws_sink = Arc::new(Mutex::new(ws_sink));
    let ws_sink_clone = Arc::clone(&ws_sink);

    // Spawn the write task: reads events from the channel, serializes, and sends.
    let write_task = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let json = match serde_json::to_string(&event) {
                Ok(j) => j,
                Err(e) => {
                    error!(error = %e, "Failed to serialize WsEvent");
                    continue;
                }
            };
            let mut sink = ws_sink_clone.lock().await;
            if let Err(e) = sink.send(Message::Text(json.into())).await {
                debug!(error = %e, "Failed to send WebSocket message (client likely disconnected)");
                break;
            }
        }
    });

    let ws_sender = WebSocketSender::new(event_tx.clone());

    // Per-connection cancellation token — reset on each new agent turn.
    let cancel_token = Arc::new(tokio::sync::Mutex::new(CancellationToken::new()));

    // Track whether an agent turn is currently running.
    let turn_running = Arc::new(tokio::sync::Mutex::new(false));

    // Per-connection agent context state.
    // Load the most recent non-archived conversation from SQLite so the agent
    // retains context across WebSocket reconnections.
    let (initial_conversation, initial_conversation_id) = {
        let db = state.db.lock().unwrap();
        let conv_id: Option<String> = db
            .conn()
            .query_row(
                "SELECT id FROM conversations WHERE archived = 0 ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        if let Some(ref cid) = conv_id {
            let mut stmt = db
                .conn()
                .prepare(
                    "SELECT role, content, metadata FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
                )
                .unwrap();

            let messages: Vec<crate::llm::ChatMessage> = stmt
                .query_map(rusqlite::params![cid], |row| {
                    let role: String = row.get(0)?;
                    let content: Option<String> = row.get(1)?;
                    let metadata: Option<String> = row.get(2)?;
                    Ok((role, content, metadata))
                })
                .unwrap()
                .filter_map(|r| r.ok())
                .filter_map(|(role, content, metadata)| {
                    match role.as_str() {
                        "user" => Some(crate::llm::ChatMessage::user(content.as_deref().unwrap_or(""))),
                        "assistant" => {
                            // Check if this assistant message had tool calls (stored in metadata)
                            if let Some(ref meta) = metadata {
                                if let Ok(tool_calls) = serde_json::from_str::<Vec<crate::llm::ChatToolCall>>(meta) {
                                    if !tool_calls.is_empty() {
                                        return Some(crate::llm::ChatMessage::assistant_tool_calls(tool_calls));
                                    }
                                }
                            }
                            Some(crate::llm::ChatMessage::assistant(content.as_deref().unwrap_or("")))
                        }
                        "tool" => {
                            // Tool results need a tool_call_id; we stored it in metadata or can skip
                            // For simplicity, skip tool messages during restore — the assistant's
                            // text responses capture the essential context
                            None
                        }
                        _ => None, // Skip system messages (rebuilt fresh each turn)
                    }
                })
                .collect();

            info!(conversation_id = %cid, message_count = messages.len(), "Restored conversation from database");
            (messages, cid.clone())
        } else {
            (Vec::new(), uuid::Uuid::new_v4().to_string())
        }
    };

    let conversation: Arc<tokio::sync::Mutex<Vec<crate::llm::ChatMessage>>> =
        Arc::new(tokio::sync::Mutex::new(initial_conversation));
    let conversation_id: Arc<tokio::sync::Mutex<String>> =
        Arc::new(tokio::sync::Mutex::new(initial_conversation_id));

    // Send initial idle status
    let _ = ws_sender
        .send(WsEvent::Status {
            state: AgentState::Idle,
        })
        .await;

    // Process incoming messages
    while let Some(msg_result) = ws_source.next().await {
        let msg = match msg_result {
            Ok(m) => m,
            Err(e) => {
                debug!(error = %e, "WebSocket read error");
                break;
            }
        };

        match msg {
            Message::Text(text) => {
                let text_str: &str = text.as_ref();
                let client_msg: ClientMessage = match serde_json::from_str(text_str) {
                    Ok(m) => m,
                    Err(e) => {
                        warn!(error = %e, raw = %text_str, "Failed to parse client message");
                        let _ = ws_sender
                            .send(WsEvent::Error {
                                message: format!("Invalid message format: {}", e),
                            })
                            .await;
                        continue;
                    }
                };

                match client_msg {
                    ClientMessage::UserMessage { content } => {
                        handle_user_message(
                            content,
                            &state,
                            &ws_sender,
                            &cancel_token,
                            &turn_running,
                            &conversation,
                            &conversation_id,
                        )
                        .await;
                    }
                    ClientMessage::Cancel => {
                        handle_cancel(&cancel_token).await;
                    }
                    ClientMessage::SwitchModel { profile } => {
                        handle_switch_model(&profile, &state, &ws_sender).await;
                    }
                    ClientMessage::NewConversation => {
                        handle_new_conversation(
                            &state,
                            &ws_sender,
                            &conversation,
                            &conversation_id,
                        )
                        .await;
                    }
                    ClientMessage::Undo {
                        entity_type,
                        entity_id,
                    } => {
                        handle_undo(&entity_type, &entity_id, &state, &ws_sender).await;
                    }
                    ClientMessage::UpdateConfig { key, value } => {
                        handle_update_config(&key, &value, &state, &ws_sender).await;
                    }
                    ClientMessage::TestConnection => {
                        handle_test_connection(&state, &ws_sender).await;
                    }
                    ClientMessage::ReportStUrl { url } => {
                        info!(url = %url, "Frontend reported SillyTavern URL — updating st_base_url");
                        // Store it in the config table for use by the ST client on next turn
                        let key = "st_base_url".to_string();
                        let value_str = serde_json::to_string(&url).unwrap_or_default();
                        let db = state.db.lock().unwrap();
                        let _ = db.conn().execute(
                            "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
                            rusqlite::params![&key, &value_str],
                        );
                    }
                }
            }
            Message::Close(_) => {
                info!("Client sent close frame");
                break;
            }
            Message::Ping(data) => {
                let mut sink = ws_sink.lock().await;
                let _ = sink.send(Message::Pong(data)).await;
            }
            _ => {
                // Ignore binary frames, pong, etc.
            }
        }
    }

    // Cancel any running turn on disconnect
    {
        let token = cancel_token.lock().await;
        token.cancel();
    }

    // Drop the event sender to signal the write task to stop
    drop(event_tx);
    let _ = write_task.await;

    Ok(())
}

/// Handle a user message — start a new agent turn.
async fn handle_user_message(
    content: String,
    state: &Arc<AppState>,
    ws_sender: &WebSocketSender,
    cancel_token: &Arc<tokio::sync::Mutex<CancellationToken>>,
    turn_running: &Arc<tokio::sync::Mutex<bool>>,
    conversation: &Arc<tokio::sync::Mutex<Vec<crate::llm::ChatMessage>>>,
    conversation_id: &Arc<tokio::sync::Mutex<String>>,
) {
    // Check if a turn is already running
    {
        let running = turn_running.lock().await;
        if *running {
            let _ = ws_sender
                .send(WsEvent::Error {
                    message: "An agent turn is already in progress. Send 'cancel' first."
                        .to_string(),
                })
                .await;
            return;
        }
    }

    // Reset the cancellation token for this new turn
    let new_token = CancellationToken::new();
    {
        let mut token = cancel_token.lock().await;
        *token = new_token.clone();
    }

    // Mark turn as running
    {
        let mut running = turn_running.lock().await;
        *running = true;
    }

    let state = Arc::clone(state);
    let ws_sender = ws_sender.clone();
    let cancel_token_clone = new_token;
    let turn_running = Arc::clone(turn_running);
    let conversation = Arc::clone(conversation);
    let conversation_id = Arc::clone(conversation_id);

    // Spawn the agent turn in a separate task so we can continue
    // processing cancel messages while it runs.
    tokio::spawn(async move {
        let conv_id = conversation_id.lock().await.clone();

        // Load the post-card prompt from the database config table (if set)
        let post_card_prompt = {
            let db_guard = state.db.lock().unwrap();
            db_guard
                .conn()
                .query_row(
                    "SELECT value FROM config WHERE key = 'post_card_prompt'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .ok()
                .and_then(|v| serde_json::from_str::<String>(&v).ok().or(Some(v)))
                .unwrap_or_default()
        };

        // Build a fresh AgentContext for this turn using ENI's base personality prompt
        let context_builder = ContextBuilder::new(
            ENI_SYSTEM_PROMPT.to_string(),
            post_card_prompt,
            state.config.default_model().map(|p| p.max_tokens as usize).unwrap_or(4096),
        );

        // Open a fresh DB connection for this turn
        let db = match Database::open(&state.config.db_path) {
            Ok(db) => db,
            Err(e) => {
                error!(error = %e, "Failed to open database for agent turn");
                let _ = ws_sender
                    .send(WsEvent::Error {
                        message: format!("Database error: {}", e),
                    })
                    .await;
                let mut running = turn_running.lock().await;
                *running = false;
                return;
            }
        };

        // Get the default model profile for the LLM client.
        // First check the database for user-configured values (set via the UI),
        // then fall back to the TOML config file, then hardcoded defaults.
        let profile = {
            let db_guard = state.db.lock().unwrap();
            let get_config = |key: &str| -> Option<String> {
                db_guard
                    .conn()
                    .query_row(
                        "SELECT value FROM config WHERE key = ?1",
                        rusqlite::params![key],
                        |row| row.get::<_, String>(0),
                    )
                    .ok()
                    .and_then(|v| serde_json::from_str::<String>(&v).ok().or(Some(v)))
            };

            let base_url = get_config("model_profile.baseUrl");
            let api_key = get_config("model_profile.apiKey");
            let model = get_config("model_profile.model");
            let temperature = get_config("model_profile.temperature")
                .and_then(|v| v.parse::<f64>().ok());
            let max_tokens = get_config("model_profile.maxTokens")
                .and_then(|v| v.parse::<u32>().ok());

            // Use DB values if any model_profile fields are set, otherwise fall back to config
            let config_profile = state.config.default_model().cloned();

            let fallback = config_profile.unwrap_or(crate::config::ModelProfile {
                name: "default".to_string(),
                base_url: "http://localhost:11434/v1".to_string(),
                api_key: "none".to_string(),
                model: "llama3".to_string(),
                temperature: 0.7,
                max_tokens: 4096,
                is_default: true,
            });

            crate::config::ModelProfile {
                name: "default".to_string(),
                base_url: base_url.unwrap_or(fallback.base_url),
                api_key: api_key.unwrap_or(fallback.api_key),
                model: model.unwrap_or(fallback.model),
                temperature: temperature.unwrap_or(fallback.temperature),
                max_tokens: max_tokens.unwrap_or(fallback.max_tokens),
                is_default: true,
            }
        };

        let llm_client = LlmClient::new(profile);

        // Build tool dispatcher with all tools registered
        let tool_dispatcher = {
            let mut dispatcher = ToolDispatcher::new();

            // Create a shared StClient for character/persona tools
            // First check the database for user-configured ST URL (set via UI or auto-reported),
            // then fall back to the TOML config file default.
            let st_config = {
                let db_guard = state.db.lock().unwrap();
                let db_st_url = db_guard
                    .conn()
                    .query_row(
                        "SELECT value FROM config WHERE key = ?1",
                        rusqlite::params!["st_base_url"],
                        |row| row.get::<_, String>(0),
                    )
                    .ok()
                    .and_then(|v| serde_json::from_str::<String>(&v).ok().or(Some(v)));

                let db_api_key = db_guard
                    .conn()
                    .query_row(
                        "SELECT value FROM config WHERE key = ?1",
                        rusqlite::params!["st_api_key"],
                        |row| row.get::<_, String>(0),
                    )
                    .ok()
                    .and_then(|v| serde_json::from_str::<String>(&v).ok().or(Some(v)));

                crate::config::StConfig {
                    base_url: db_st_url.unwrap_or_else(|| state.config.sillytavern.base_url.clone()),
                    api_key: db_api_key.or_else(|| state.config.sillytavern.api_key.clone()),
                }
            };

            info!(url = %st_config.base_url, "Creating ST client with URL");

            let st_client = match StClient::new(&st_config).await {
                Ok(client) => Arc::new(tokio::sync::Mutex::new(client)),
                Err(e) => {
                    warn!(error = %e, "Failed to create ST client; character/persona tools will be unavailable");
                    let fallback = StClient::new(&crate::config::StConfig::default()).await
                        .unwrap_or_else(|_| panic!("Failed to create fallback ST client"));
                    Arc::new(tokio::sync::Mutex::new(fallback))
                }
            };

            // Shared DB for tools (using the app-level shared DB)
            let tool_db = Arc::clone(&state.db);
            let version_store = Arc::clone(&state.version_store);
            let search_index = Arc::clone(&state.search_index);

            // Get the event sender for tools that need to send WS events
            let event_tx = ws_sender.inner().clone();

            // Register all tools
            dispatcher.register(Box::new(ReadCharacterTool::new(Arc::clone(&st_client))));
            dispatcher.register(Box::new(WriteCharacterTool::new(
                Arc::clone(&st_client),
                Arc::clone(&version_store),
                event_tx.clone(),
            )));
            dispatcher.register(Box::new(ListCharactersTool::new(Arc::clone(&st_client))));
            dispatcher.register(Box::new(ExportCardTool::new(Arc::clone(&st_client), None)));
            dispatcher.register(Box::new(ReadPersonaTool::new(Arc::clone(&st_client))));
            dispatcher.register(Box::new(WritePersonaTool::new(
                Arc::clone(&st_client),
                Arc::clone(&version_store),
                event_tx.clone(),
            )));
            dispatcher.register(Box::new(ListPersonasTool::new(Arc::clone(&st_client))));
            dispatcher.register(Box::new(ReadPostHistoryTool::new(Arc::clone(&tool_db))));
            dispatcher.register(Box::new(WritePostHistoryTool::new(
                Arc::clone(&tool_db),
                Arc::clone(&version_store),
                event_tx.clone(),
            )));
            dispatcher.register(Box::new(ReadWorldEntriesTool::new(Arc::clone(&tool_db))));
            dispatcher.register(Box::new(WriteWorldEntryTool::new(
                Arc::clone(&tool_db),
                Arc::clone(&version_store),
                event_tx.clone(),
            )));
            dispatcher.register(Box::new(SearchLocalTool::new(Arc::clone(&search_index))));
            dispatcher.register(Box::new(SearchWikiTool::new(None)));
            dispatcher.register(Box::new(FetchWikiPageTool::new(None)));
            dispatcher.register(Box::new(ShowPreviewTool::new(event_tx.clone())));
            dispatcher.register(Box::new(CreateProjectTool::new(Arc::clone(&tool_db))));
            dispatcher.register(Box::new(ManageTasksTool::new(Arc::clone(&tool_db))));
            dispatcher.register(Box::new(UndoChangeTool::new(
                Arc::clone(&version_store),
                Arc::clone(&st_client),
                Arc::clone(&tool_db),
            )));
            dispatcher.register(Box::new(ListVersionsTool::new(Arc::clone(&version_store))));

            dispatcher
        };

        // Take the current conversation history
        let conv_history = conversation.lock().await.clone();

        let mut ctx = AgentContext {
            conversation: conv_history,
            conversation_id: conv_id,
            context_builder,
            llm_client,
            tool_dispatcher,
            config: state.config.clone(),
            db,
            relevant_chunks: Vec::new(),
            lorebook: crate::lorebook::defaults::build_default_lorebook(),
        };

        // Run the agent turn
        let tx = ws_sender.inner().clone();
        if let Err(e) = agent::run_turn(&mut ctx, content, &tx, &cancel_token_clone).await {
            error!(error = %e, "Agent turn failed");
            let _ = ws_sender
                .send(WsEvent::Error {
                    message: format!("Agent error: {}", e),
                })
                .await;
            let _ = ws_sender
                .send(WsEvent::Status {
                    state: AgentState::Idle,
                })
                .await;
        }

        // Update the shared conversation with the new state
        {
            let mut conv = conversation.lock().await;
            *conv = ctx.conversation;
        }

        // Mark turn as complete
        {
            let mut running = turn_running.lock().await;
            *running = false;
        }
    });
}

/// Handle a cancel request — trigger the cancellation token.
async fn handle_cancel(cancel_token: &Arc<tokio::sync::Mutex<CancellationToken>>) {
    let token = cancel_token.lock().await;
    token.cancel();
    info!("Cancel signal sent to active agent turn");
}

/// Handle a model switch request.
async fn handle_switch_model(profile_name: &str, state: &Arc<AppState>, ws_sender: &WebSocketSender) {
    // The frontend sends the model name (e.g., "claude-opus-4.6") from the dropdown.
    // We need to persist this choice to the database so it's used on the next agent turn.
    // First check if it matches a named profile in the TOML config.
    if let Some(profile) = state.config.model_by_name(profile_name) {
        // Full profile match — persist all fields
        {
            let db = state.db.lock().unwrap();
            let _ = db.conn().execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
                rusqlite::params!["model_profile.model", serde_json::to_string(&profile.model).unwrap_or_default()],
            );
            let _ = db.conn().execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
                rusqlite::params!["model_profile.baseUrl", serde_json::to_string(&profile.base_url).unwrap_or_default()],
            );
            let _ = db.conn().execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
                rusqlite::params!["model_profile.apiKey", serde_json::to_string(&profile.api_key).unwrap_or_default()],
            );
            let _ = db.conn().execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
                rusqlite::params!["model_profile.temperature", profile.temperature.to_string()],
            );
            let _ = db.conn().execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
                rusqlite::params!["model_profile.maxTokens", profile.max_tokens.to_string()],
            );
        } // MutexGuard dropped here before await

        state.llm_client.switch_profile(profile.clone()).await;
        info!(profile = %profile_name, "Switched to named model profile");
    } else {
        // Not a named profile — just update the model field in the DB.
        // This handles the case where the user picks a model from the /models endpoint
        // dropdown (e.g., "claude-opus-4.6") that isn't a full TOML profile.
        {
            let db = state.db.lock().unwrap();
            let _ = db.conn().execute(
                "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
                rusqlite::params!["model_profile.model", serde_json::to_string(profile_name).unwrap_or_default()],
            );
        } // MutexGuard dropped here
        info!(model = %profile_name, "Switched model (persisted to DB)");
    }

    // Confirm the switch to the frontend (do NOT trigger fetchConfig re-fetch)
    let _ = ws_sender
        .send(WsEvent::ConfigUpdated {
            key: "model_switched".to_string(),
        })
        .await;
}

/// Handle a new conversation request — archive current and reset.
async fn handle_new_conversation(
    state: &Arc<AppState>,
    ws_sender: &WebSocketSender,
    conversation: &Arc<tokio::sync::Mutex<Vec<crate::llm::ChatMessage>>>,
    conversation_id: &Arc<tokio::sync::Mutex<String>>,
) {
    // Archive the current conversation by marking it in the database
    let old_id = conversation_id.lock().await.clone();

    // Perform the DB operation in a block so the MutexGuard is dropped before any await
    let archive_result = {
        let db = state.db.lock().unwrap();
        db.conn().execute(
            "UPDATE conversations SET archived = 1 WHERE id = ?1",
            rusqlite::params![&old_id],
        )
    };
    if let Err(e) = archive_result {
        warn!(error = %e, "Failed to archive conversation (may not exist yet)");
    }

    // Reset conversation state
    {
        let mut conv = conversation.lock().await;
        conv.clear();
    }
    {
        let mut id = conversation_id.lock().await;
        *id = uuid::Uuid::new_v4().to_string();
    }

    info!(old_id = %old_id, "Conversation archived, starting new conversation");

    let _ = ws_sender
        .send(WsEvent::SystemMessage {
            content: "New conversation started.".to_string(),
        })
        .await;
}

/// Handle an undo request — pop the latest version for the specified entity.
async fn handle_undo(
    entity_type: &str,
    entity_id: &str,
    state: &Arc<AppState>,
    ws_sender: &WebSocketSender,
) {
    match state.version_store.undo(entity_type, entity_id) {
        Ok(Some(data)) => {
            info!(
                entity_type = %entity_type,
                entity_id = %entity_id,
                "Undo successful"
            );
            let _ = ws_sender
                .send(WsEvent::SystemMessage {
                    content: format!(
                        "Undo successful for {} '{}'. Previous state restored.",
                        entity_type, entity_id
                    ),
                })
                .await;
            // Also send the restored data as a preview
            let _ = ws_sender
                .send(WsEvent::Preview {
                    tab: entity_type.to_string(),
                    data,
                })
                .await;
        }
        Ok(None) => {
            let _ = ws_sender
                .send(WsEvent::Error {
                    message: format!(
                        "No undo history available for {} '{}'.",
                        entity_type, entity_id
                    ),
                })
                .await;
        }
        Err(e) => {
            error!(error = %e, "Undo operation failed");
            let _ = ws_sender
                .send(WsEvent::Error {
                    message: format!("Undo failed: {}", e),
                })
                .await;
        }
    }
}

/// Handle a config update request.
async fn handle_update_config(
    key: &str,
    value: &serde_json::Value,
    state: &Arc<AppState>,
    ws_sender: &WebSocketSender,
) {
    // Store the config value in the SQLite config table
    let value_str = serde_json::to_string(value).unwrap_or_default();
    let key_owned = key.to_string();

    // Perform the DB operation in a block so the MutexGuard is dropped before any await
    let db_result = {
        let db = state.db.lock().unwrap();
        db.conn().execute(
            "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
            rusqlite::params![&key_owned, &value_str],
        )
    };

    match db_result {
        Ok(_) => {
            info!(key = %key, "Config updated");
        }
        Err(e) => {
            error!(key = %key, error = %e, "Failed to update config");
            let _ = ws_sender
                .send(WsEvent::Error {
                    message: format!("Failed to update config: {}", e),
                })
                .await;
            return;
        }
    }

    let _ = ws_sender
        .send(WsEvent::ConfigUpdated {
            key: key.to_string(),
        })
        .await;
}

/// Handle a test connection request — verify the LLM API is reachable with current settings.
async fn handle_test_connection(state: &Arc<AppState>, ws_sender: &WebSocketSender) {
    // Read the model profile from the database (same logic as handle_user_message)
    let profile = {
        let db_guard = state.db.lock().unwrap();
        let get_config = |key: &str| -> Option<String> {
            db_guard
                .conn()
                .query_row(
                    "SELECT value FROM config WHERE key = ?1",
                    rusqlite::params![key],
                    |row| row.get::<_, String>(0),
                )
                .ok()
                .and_then(|v| serde_json::from_str::<String>(&v).ok().or(Some(v)))
        };

        let base_url = get_config("model_profile.baseUrl");
        let api_key = get_config("model_profile.apiKey");
        let model = get_config("model_profile.model");

        let config_profile = state.config.default_model().cloned();
        let fallback = config_profile.unwrap_or(crate::config::ModelProfile {
            name: "default".to_string(),
            base_url: "http://localhost:11434/v1".to_string(),
            api_key: "none".to_string(),
            model: "llama3".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            is_default: true,
        });

        crate::config::ModelProfile {
            name: "default".to_string(),
            base_url: base_url.unwrap_or(fallback.base_url),
            api_key: api_key.unwrap_or(fallback.api_key),
            model: model.unwrap_or(fallback.model),
            temperature: fallback.temperature,
            max_tokens: fallback.max_tokens,
            is_default: true,
        }
    };

    info!(base_url = %profile.base_url, model = %profile.model, "Testing LLM connection");

    // Make a minimal chat completion request to verify connectivity
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap();

    let url = format!("{}/chat/completions", profile.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": profile.model,
        "messages": [{"role": "user", "content": "Hi"}],
        "max_tokens": 1,
        "stream": false,
    });

    match client
        .post(&url)
        .header("Authorization", format!("Bearer {}", profile.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                info!("LLM connection test successful");
                let _ = ws_sender
                    .send(WsEvent::SystemMessage {
                        content: format!(
                            "✓ Connection successful! Model '{}' is reachable at {}",
                            profile.model, profile.base_url
                        ),
                    })
                    .await;
            } else {
                let body_text = response.text().await.unwrap_or_default();
                let msg = format!(
                    "✗ Connection failed (HTTP {}): {}",
                    status.as_u16(),
                    body_text.chars().take(200).collect::<String>()
                );
                warn!(status = %status, "LLM connection test failed");
                let _ = ws_sender.send(WsEvent::Error { message: msg }).await;
            }
        }
        Err(e) => {
            let msg = if e.is_timeout() {
                format!("✗ Connection timed out after 15s: {}", profile.base_url)
            } else if e.is_connect() {
                format!("✗ Cannot reach server at {}", profile.base_url)
            } else {
                format!("✗ Connection error: {}", e)
            };
            error!(error = %e, "LLM connection test error");
            let _ = ws_sender.send(WsEvent::Error { message: msg }).await;
        }
    }
}
