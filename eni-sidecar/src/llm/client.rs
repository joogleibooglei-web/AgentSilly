//! LLM client implementation — streaming chat completions via SSE.

use std::sync::Arc;
use tokio::sync::RwLock;

use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use tracing::{debug, error, warn};

use crate::config::ModelProfile;

use super::types::*;

/// Accumulator for building tool calls from streamed deltas.
#[derive(Debug, Default)]
struct ToolCallAccumulator {
    id: String,
    name: String,
    arguments: String,
}

/// The LLM client for making streaming chat completion requests.
///
/// Holds a shared reqwest HTTP client and the active model profile.
/// The active profile can be swapped at runtime via `switch_profile`.
pub struct LlmClient {
    http: Client,
    active_profile: Arc<RwLock<ModelProfile>>,
}

impl LlmClient {
    /// Create a new LLM client with the given model profile.
    pub fn new(profile: ModelProfile) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            http,
            active_profile: Arc::new(RwLock::new(profile)),
        }
    }

    /// Switch the active model profile at runtime.
    /// Subsequent calls will use the new profile without restarting.
    pub async fn switch_profile(&self, profile: ModelProfile) {
        let mut active = self.active_profile.write().await;
        *active = profile;
    }

    /// Get the name of the currently active model profile.
    pub async fn active_profile_name(&self) -> String {
        let profile = self.active_profile.read().await;
        profile.name.clone()
    }

    /// Get the model identifier of the currently active profile.
    pub async fn active_model(&self) -> String {
        let profile = self.active_profile.read().await;
        profile.model.clone()
    }

    /// Perform a streaming chat completion request.
    ///
    /// Sends the messages and tool definitions to the configured LLM API endpoint,
    /// parses the SSE stream, and returns either accumulated text or tool calls.
    ///
    /// The `on_token` callback is invoked for each text delta as it arrives,
    /// allowing the caller to relay tokens to the frontend in real-time.
    pub async fn chat_completion_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        on_token: Option<&TokenCallback>,
        on_thinking: Option<&ThinkingCallback>,
    ) -> Result<LlmResponse> {
        // Snapshot the active profile for this request
        let profile = self.active_profile.read().await.clone();

        // Build the request body
        let mut body = serde_json::json!({
            "model": profile.model,
            "messages": messages,
            "stream": true,
            "temperature": profile.temperature,
        });

        // Only include tools if there are any defined
        if !tools.is_empty() {
            body["tools"] = serde_json::to_value(tools)?;
        }

        // Include max_tokens if set
        if profile.max_tokens > 0 {
            body["max_tokens"] = serde_json::json!(profile.max_tokens);
        }

        let url = format!("{}/chat/completions", profile.base_url.trim_end_matches('/'));
        debug!(url = %url, model = %profile.model, "Sending chat completion request");

        // Send the request
        let response = self
            .http
            .post(&url)
            .bearer_auth(&profile.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to send chat completion request to {}", url))?;

        // Check for HTTP errors
        let status = response.status();
        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error body".to_string());
            anyhow::bail!(
                "LLM API returned HTTP {}: {}",
                status.as_u16(),
                error_body
            );
        }

        // Parse the SSE stream
        self.process_sse_stream(response, on_token, on_thinking).await
    }

    /// Parse the SSE event stream from the chat completion response.
    ///
    /// Accumulates text content tokens and tool call deltas across multiple
    /// SSE events, handling partial JSON assembly for tool call arguments.
    async fn process_sse_stream(
        &self,
        response: reqwest::Response,
        on_token: Option<&TokenCallback>,
        on_thinking: Option<&ThinkingCallback>,
    ) -> Result<LlmResponse> {
        use eventsource_stream::Eventsource;

        let mut text_content = String::new();
        let mut tool_accumulators: Vec<ToolCallAccumulator> = Vec::new();
        let mut has_tool_calls = false;

        // Convert the response body into an SSE event stream
        let mut stream = response.bytes_stream().eventsource();

        while let Some(event_result) = stream.next().await {
            let event = match event_result {
                Ok(ev) => ev,
                Err(e) => {
                    // eventsource-stream errors are typically connection issues
                    warn!(error = %e, "SSE stream error");
                    continue;
                }
            };

            // Handle the [DONE] sentinel
            let data = event.data.trim();
            if data == "[DONE]" {
                debug!("SSE stream complete ([DONE] received)");
                break;
            }

            // Skip empty data
            if data.is_empty() {
                continue;
            }

            // Parse the SSE data as a stream chunk
            let chunk: StreamChunk = match serde_json::from_str(data) {
                Ok(c) => c,
                Err(e) => {
                    warn!(error = %e, data = %data, "Failed to parse SSE chunk");
                    continue;
                }
            };

            // Process each choice in the chunk
            for choice in &chunk.choices {
                // Accumulate text content
                if let Some(ref content) = choice.delta.content {
                    text_content.push_str(content);
                    // Relay token to callback
                    if let Some(cb) = on_token {
                        cb(content);
                    }
                }

                // Accumulate reasoning/thinking content
                if let Some(ref reasoning) = choice.delta.reasoning_content {
                    if let Some(cb) = on_thinking {
                        cb(reasoning);
                    }
                }

                // Accumulate tool call deltas
                if let Some(ref tool_call_deltas) = choice.delta.tool_calls {
                    has_tool_calls = true;
                    for delta in tool_call_deltas {
                        // Ensure we have an accumulator for this index
                        while tool_accumulators.len() <= delta.index {
                            tool_accumulators.push(ToolCallAccumulator::default());
                        }

                        let acc = &mut tool_accumulators[delta.index];

                        // Set the ID if provided (comes in the first delta for this index)
                        if let Some(ref id) = delta.id {
                            acc.id = id.clone();
                        }

                        // Accumulate function name and arguments
                        if let Some(ref func) = delta.function {
                            if let Some(ref name) = func.name {
                                acc.name = name.clone();
                            }
                            if let Some(ref args) = func.arguments {
                                acc.arguments.push_str(args);
                            }
                        }
                    }
                }
            }
        }

        // Determine the response type
        if has_tool_calls && !tool_accumulators.is_empty() {
            let tool_calls: Vec<ToolCall> = tool_accumulators
                .into_iter()
                .filter(|acc| !acc.name.is_empty())
                .map(|acc| {
                    let arguments = match serde_json::from_str(&acc.arguments) {
                        Ok(v) => v,
                        Err(e) => {
                            error!(
                                tool = %acc.name,
                                raw_args = %acc.arguments,
                                error = %e,
                                "Failed to parse tool call arguments as JSON"
                            );
                            // Return the raw string wrapped in a JSON value so the
                            // dispatcher can report a validation error
                            serde_json::Value::String(acc.arguments.clone())
                        }
                    };

                    ToolCall {
                        id: acc.id,
                        name: acc.name,
                        arguments,
                        arguments_raw: acc.arguments,
                    }
                })
                .collect();

            if tool_calls.is_empty() {
                // Edge case: had tool call deltas but none with a name
                Ok(LlmResponse::Text(text_content))
            } else {
                Ok(LlmResponse::ToolCalls(tool_calls))
            }
        } else {
            Ok(LlmResponse::Text(text_content))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelProfile;
    use axum::{
        body::Body,
        http::{header, StatusCode},
        response::Response,
        routing::post,
        Router,
    };
    use tokio::net::TcpListener;

    /// Helper to create a test ModelProfile pointing at a local server.
    fn test_profile(port: u16) -> ModelProfile {
        ModelProfile {
            name: "test".to_string(),
            base_url: format!("http://127.0.0.1:{}/v1", port),
            api_key: "test-key".to_string(),
            model: "test-model".to_string(),
            temperature: 0.7,
            max_tokens: 1024,
            is_default: true,
        }
    }

    /// Helper to build an SSE response body from a list of SSE data lines.
    fn sse_body(events: &[&str]) -> String {
        let mut body = String::new();
        for event in events {
            body.push_str(&format!("data: {}\n\n", event));
        }
        body
    }

    /// Start a mock server that responds with the given SSE body.
    /// Returns the port the server is listening on.
    async fn start_mock_server(sse_response: String) -> u16 {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move || {
                let body = sse_response.clone();
                async move {
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "text/event-stream")
                        .body(Body::from(body))
                        .unwrap()
                }
            }),
        );

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        addr.port()
    }

    /// Start a mock server that returns an HTTP error status.
    async fn start_error_server(status_code: u16, error_body: &str) -> u16 {
        let error_body = error_body.to_string();
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move || {
                let body = error_body.clone();
                async move {
                    Response::builder()
                        .status(StatusCode::from_u16(status_code).unwrap())
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(body))
                        .unwrap()
                }
            }),
        );

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        addr.port()
    }

    // =========================================================================
    // Test 1: Parsing of streamed text tokens
    // =========================================================================

    #[tokio::test]
    async fn test_parse_streamed_text_tokens() {
        // Simulate an SSE stream with multiple text content deltas
        let sse = sse_body(&[
            r#"{"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":" world"},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":"!"},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
            "[DONE]",
        ]);

        let port = start_mock_server(sse).await;
        let client = LlmClient::new(test_profile(port));

        let messages = vec![ChatMessage::user("Hi")];
        let result = client
            .chat_completion_stream(&messages, &[], None, None)
            .await
            .unwrap();

        match result {
            LlmResponse::Text(text) => {
                assert_eq!(text, "Hello world!");
            }
            LlmResponse::ToolCalls(_) => panic!("Expected Text response, got ToolCalls"),
        }
    }

    #[tokio::test]
    async fn test_parse_streamed_text_with_token_callback() {
        // Verify that the token callback is invoked for each delta
        let sse = sse_body(&[
            r#"{"choices":[{"delta":{"content":"One"},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":" Two"},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":" Three"},"finish_reason":null}]}"#,
            "[DONE]",
        ]);

        let port = start_mock_server(sse).await;
        let client = LlmClient::new(test_profile(port));

        let tokens = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let tokens_clone = tokens.clone();
        let callback: TokenCallback = Box::new(move |token: &str| {
            tokens_clone.lock().unwrap().push(token.to_string());
        });

        let messages = vec![ChatMessage::user("Count")];
        let result = client
            .chat_completion_stream(&messages, &[], Some(&callback), None)
            .await
            .unwrap();

        match result {
            LlmResponse::Text(text) => {
                assert_eq!(text, "One Two Three");
            }
            LlmResponse::ToolCalls(_) => panic!("Expected Text response, got ToolCalls"),
        }

        let received_tokens = tokens.lock().unwrap();
        assert_eq!(received_tokens.len(), 3);
        assert_eq!(received_tokens[0], "One");
        assert_eq!(received_tokens[1], " Two");
        assert_eq!(received_tokens[2], " Three");
    }

    #[tokio::test]
    async fn test_parse_empty_text_response() {
        // Stream with only a finish_reason and [DONE], no content
        let sse = sse_body(&[
            r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
            "[DONE]",
        ]);

        let port = start_mock_server(sse).await;
        let client = LlmClient::new(test_profile(port));

        let messages = vec![ChatMessage::user("Hi")];
        let result = client
            .chat_completion_stream(&messages, &[], None, None)
            .await
            .unwrap();

        match result {
            LlmResponse::Text(text) => {
                assert_eq!(text, "");
            }
            LlmResponse::ToolCalls(_) => panic!("Expected Text response, got ToolCalls"),
        }
    }

    // =========================================================================
    // Test 2: Parsing of streamed tool_call chunks with partial JSON
    // =========================================================================

    #[tokio::test]
    async fn test_parse_single_tool_call() {
        // Simulate a tool call arriving in multiple SSE chunks with partial JSON arguments
        let sse = sse_body(&[
            // First chunk: tool call ID and function name
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc123","type":"function","function":{"name":"read_character","arguments":""}}]},"finish_reason":null}]}"#,
            // Second chunk: partial arguments
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"name\":"}}]},"finish_reason":null}]}"#,
            // Third chunk: more arguments
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"Kael\","}}]},"finish_reason":null}]}"#,
            // Fourth chunk: finish arguments
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"fields\":[\"description\"]}"}}]},"finish_reason":null}]}"#,
            // Finish
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
            "[DONE]",
        ]);

        let port = start_mock_server(sse).await;
        let client = LlmClient::new(test_profile(port));

        let messages = vec![ChatMessage::user("Read Kael's description")];
        let result = client
            .chat_completion_stream(&messages, &[], None, None)
            .await
            .unwrap();

        match result {
            LlmResponse::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].id, "call_abc123");
                assert_eq!(calls[0].name, "read_character");
                assert_eq!(
                    calls[0].arguments,
                    serde_json::json!({"name": "Kael", "fields": ["description"]})
                );
                assert_eq!(
                    calls[0].arguments_raw,
                    r#"{"name":"Kael","fields":["description"]}"#
                );
            }
            LlmResponse::Text(_) => panic!("Expected ToolCalls response, got Text"),
        }
    }

    #[tokio::test]
    async fn test_parse_multiple_tool_calls() {
        // Simulate two tool calls arriving interleaved in the stream
        let sse = sse_body(&[
            // First tool call starts
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_001","type":"function","function":{"name":"write_character","arguments":""}}]},"finish_reason":null}]}"#,
            // Second tool call starts
            r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_002","type":"function","function":{"name":"show_preview","arguments":""}}]},"finish_reason":null}]}"#,
            // First tool call arguments
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"name\":\"Kael\"}"}}]},"finish_reason":null}]}"#,
            // Second tool call arguments
            r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"{\"type\":\"character\"}"}}]},"finish_reason":null}]}"#,
            // Finish
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
            "[DONE]",
        ]);

        let port = start_mock_server(sse).await;
        let client = LlmClient::new(test_profile(port));

        let messages = vec![ChatMessage::user("Create and preview")];
        let result = client
            .chat_completion_stream(&messages, &[], None, None)
            .await
            .unwrap();

        match result {
            LlmResponse::ToolCalls(calls) => {
                assert_eq!(calls.len(), 2);

                assert_eq!(calls[0].id, "call_001");
                assert_eq!(calls[0].name, "write_character");
                assert_eq!(calls[0].arguments, serde_json::json!({"name": "Kael"}));

                assert_eq!(calls[1].id, "call_002");
                assert_eq!(calls[1].name, "show_preview");
                assert_eq!(calls[1].arguments, serde_json::json!({"type": "character"}));
            }
            LlmResponse::Text(_) => panic!("Expected ToolCalls response, got Text"),
        }
    }

    #[tokio::test]
    async fn test_parse_tool_call_with_invalid_json_arguments() {
        // Simulate a tool call where the arguments don't form valid JSON
        // The client should still return the tool call with the raw string wrapped in a JSON Value::String
        let sse = sse_body(&[
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_bad","type":"function","function":{"name":"some_tool","arguments":""}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{invalid json"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
            "[DONE]",
        ]);

        let port = start_mock_server(sse).await;
        let client = LlmClient::new(test_profile(port));

        let messages = vec![ChatMessage::user("Do something")];
        let result = client
            .chat_completion_stream(&messages, &[], None, None)
            .await
            .unwrap();

        match result {
            LlmResponse::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "some_tool");
                // Invalid JSON gets wrapped as a string value
                assert_eq!(
                    calls[0].arguments,
                    serde_json::Value::String("{invalid json".to_string())
                );
                assert_eq!(calls[0].arguments_raw, "{invalid json");
            }
            LlmResponse::Text(_) => panic!("Expected ToolCalls response, got Text"),
        }
    }

    // =========================================================================
    // Test 3: Handling of error events and malformed SSE
    // =========================================================================

    #[tokio::test]
    async fn test_http_error_response() {
        // Server returns a 429 rate limit error
        let port = start_error_server(
            429,
            r#"{"error":{"message":"Rate limit exceeded","type":"rate_limit_error"}}"#,
        )
        .await;

        let client = LlmClient::new(test_profile(port));
        let messages = vec![ChatMessage::user("Hi")];
        let result = client.chat_completion_stream(&messages, &[], None, None).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("429"));
        assert!(err_msg.contains("Rate limit exceeded"));
    }

    #[tokio::test]
    async fn test_http_500_error() {
        // Server returns a 500 internal server error
        let port = start_error_server(500, "Internal Server Error").await;

        let client = LlmClient::new(test_profile(port));
        let messages = vec![ChatMessage::user("Hi")];
        let result = client.chat_completion_stream(&messages, &[], None, None).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("500"));
    }

    #[tokio::test]
    async fn test_malformed_sse_chunks_are_skipped() {
        // Mix of valid and malformed SSE data — malformed chunks should be skipped
        let sse = sse_body(&[
            r#"{"choices":[{"delta":{"content":"Good"},"finish_reason":null}]}"#,
            // Malformed JSON
            r#"not valid json at all"#,
            // Another valid chunk
            r#"{"choices":[{"delta":{"content":" stuff"},"finish_reason":null}]}"#,
            // Empty data (should be skipped)
            "",
            "[DONE]",
        ]);

        let port = start_mock_server(sse).await;
        let client = LlmClient::new(test_profile(port));

        let messages = vec![ChatMessage::user("Hi")];
        let result = client
            .chat_completion_stream(&messages, &[], None, None)
            .await
            .unwrap();

        match result {
            LlmResponse::Text(text) => {
                // Malformed chunks are skipped, valid ones are accumulated
                assert_eq!(text, "Good stuff");
            }
            LlmResponse::ToolCalls(_) => panic!("Expected Text response, got ToolCalls"),
        }
    }

    #[tokio::test]
    async fn test_done_sentinel_terminates_stream() {
        // Verify that [DONE] stops processing even if there's more data after it
        let sse = sse_body(&[
            r#"{"choices":[{"delta":{"content":"Before"},"finish_reason":null}]}"#,
            "[DONE]",
            // This should never be processed
            r#"{"choices":[{"delta":{"content":" After"},"finish_reason":null}]}"#,
        ]);

        let port = start_mock_server(sse).await;
        let client = LlmClient::new(test_profile(port));

        let messages = vec![ChatMessage::user("Hi")];
        let result = client
            .chat_completion_stream(&messages, &[], None, None)
            .await
            .unwrap();

        match result {
            LlmResponse::Text(text) => {
                assert_eq!(text, "Before");
            }
            LlmResponse::ToolCalls(_) => panic!("Expected Text response, got ToolCalls"),
        }
    }

    #[tokio::test]
    async fn test_tool_call_deltas_without_name_fallback_to_text() {
        // Edge case: tool_calls deltas arrive but none have a name set
        // This should fall back to returning text content
        let sse = sse_body(&[
            r#"{"choices":[{"delta":{"content":"Some text","tool_calls":[{"index":0,"id":"call_x","function":{"arguments":""}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
            "[DONE]",
        ]);

        let port = start_mock_server(sse).await;
        let client = LlmClient::new(test_profile(port));

        let messages = vec![ChatMessage::user("Hi")];
        let result = client
            .chat_completion_stream(&messages, &[], None, None)
            .await
            .unwrap();

        // Since no tool call has a name, it falls back to text
        match result {
            LlmResponse::Text(text) => {
                assert_eq!(text, "Some text");
            }
            LlmResponse::ToolCalls(_) => panic!("Expected Text fallback, got ToolCalls"),
        }
    }
}
