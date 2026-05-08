# Implementation Plan: Agent Loop Rework (ENI World Builder)

## Overview

Full architectural rework: a Rust sidecar (agent loop, tools, WebSocket, SQLite) paired with a Svelte frontend (ST extension sidebar panel, thin client). The sidecar is built first since the frontend is useless without it. Implementation follows the Codex-style agent loop pattern from the reference repos.

**Languages:** Rust (sidecar), TypeScript + Svelte (frontend)

## Tasks

- [x] 1. Rust sidecar scaffolding
  - [x] 1.1 Initialize cargo workspace and binary crate `eni-sidecar`
    - Create `Cargo.toml` with all dependencies from design (tokio, axum, tokio-tungstenite, serde, rusqlite, reqwest, tantivy, tiktoken-rs, chrono, uuid, toml, tracing, anyhow, async-trait, futures, eventsource-stream)
    - Create `src/main.rs` with tokio runtime, tracing subscriber init, config loading, and server startup
    - Create module structure: `src/{agent, llm, tools, context, search, versioning, ws, http, db, config}.rs` (or directories)
    - _Requirements: 9.3, 9.5_

  - [x] 1.2 Implement TOML config loading
    - Create `src/config.rs` with structs for `AppConfig`, `ModelProfile`, `StConfig`
    - Parse TOML file from `~/.config/eni-sidecar/config.toml` (or CLI-specified path)
    - Fields: LLM base_url, api_key, model, temperature, max_tokens, ST base_url, listen port, db_path
    - Support multiple named model profiles with a default marker
    - _Requirements: 9.3, 12.1, 12.2_

  - [x] 1.3 Set up SQLite database and migrations
    - Create `src/db/mod.rs` with connection pool setup (rusqlite)
    - Implement schema creation: conversations, messages, projects, tasks, world_entries, reference_documents, document_chunks, entity_versions, config, model_profiles tables (matching design schema exactly)
    - Run migrations on startup (create tables if not exist)
    - _Requirements: 7.1, 7.3_

- [x] 2. LLM client with SSE streaming
  - [x] 2.1 Implement the LLM client struct and chat completion request
    - Create `src/llm/client.rs` with `LlmClient` struct holding reqwest client and active `ModelProfile`
    - Build the OpenAI-compatible `/chat/completions` POST request body (model, messages, tools, stream: true, temperature)
    - Set bearer auth header from config api_key
    - _Requirements: 1.7, 4.2, 4.5_

  - [x] 2.2 Implement SSE stream parsing
    - Parse `text/event-stream` response using eventsource-stream crate
    - Accumulate text content tokens into `LlmResponse::Text`
    - Detect and accumulate `tool_calls` chunks into `LlmResponse::ToolCall` (handle partial JSON assembly across SSE events)
    - Handle `[DONE]` sentinel and stream errors
    - _Requirements: 2.1, 1.1_

  - [x] 2.3 Implement model profile switching
    - Add method to swap the active `ModelProfile` on the `LlmClient` at runtime
    - Ensure subsequent calls use the new profile without restarting
    - _Requirements: 12.4, 12.5_

  - [x] 2.4 Write unit tests for SSE parsing
    - Test parsing of streamed text tokens
    - Test parsing of streamed tool_call chunks with partial JSON
    - Test handling of error events and malformed SSE
    - _Requirements: 2.1_

- [x] 3. Agent loop core
  - [x] 3.1 Implement the context builder
    - Create `src/context/builder.rs` with `ContextBuilder` struct
    - Assemble messages array: system prompt (ENI personality + post-card + reference chunks) + conversation history
    - Implement token counting with tiktoken-rs for budget enforcement
    - Implement truncation: remove oldest messages when over budget, always preserve system prompt + last 4 messages
    - Format tool definitions in OpenAI function-calling format (tools array with JSON schemas)
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5_

  - [x] 3.2 Implement the tool dispatcher
    - Create `src/tools/dispatcher.rs` with `ToolDispatcher` struct and `Tool` trait (name, description, parameters_schema, validate_args, execute)
    - Route tool calls by name to registered tool implementations
    - Validate arguments against JSON schema before execution
    - Return `ToolResult` with success/error status
    - _Requirements: 3.17, 3.18_

  - [x] 3.3 Implement the agent loop (`run_turn`)
    - Create `src/agent/loop.rs` with `run_turn` function matching design pseudocode
    - Loop: build context → call LLM → if text response, stream tokens and break; if tool_call, execute tool and continue
    - Enforce iteration limit (max 15 turns) — halt with error message on exceed
    - Send WebSocket events: `token`, `tool_start`, `tool_end`, `message_complete`, `error`, `status`
    - Persist conversation to SQLite after turn completes
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 2.3_

  - [x] 3.4 Implement cancellation support
    - Accept cancel signal via a tokio CancellationToken shared with the WebSocket handler
    - On cancel: abort the active reqwest stream, halt pending tool execution, send "Generation stopped" system message
    - Return agent to idle state
    - _Requirements: 8.2, 8.3_

  - [x] 3.5 Write unit tests for context builder truncation logic
    - Test that system prompt is always preserved
    - Test that last 4 messages are always preserved
    - Test truncation removes oldest messages first
    - _Requirements: 4.3_

- [x] 4. Checkpoint — Core agent loop compiles and runs
  - Ensure all tests pass, ask the user if questions arise. If all tests pass, commit all changes, and merge with main. 

- [x] 5. Tool implementations — Character, Persona, World
  - [x] 5.1 Implement ST REST API client helper
    - Create `src/tools/st_client.rs` with HTTP client for SillyTavern REST API
    - Handle session/cookie auth and CSRF token (fetch CSRF from ST, include in headers)
    - Methods: `get_characters`, `create_character`, `edit_character`, `delete_character`, `export_character`
    - Error handling: return descriptive error if ST is unreachable
    - _Requirements: 11.3_

  - [x] 5.2 Implement `read_character` tool
    - Read one or more fields from a character card via ST REST API (`GET /api/characters/all` then filter, or use export endpoint)
    - Parameters: character name or ID, optional field filter
    - _Requirements: 3.1_

  - [x] 5.3 Implement `write_character` tool
    - Write one or more fields to a character card via ST REST API (`POST /api/characters/edit`)
    - Snapshot previous state via VersionStore before writing
    - Send `undo_available` event to frontend after successful write
    - Parameters: character name/ID, fields to update (description, personality, scenario, first_message, etc.)
    - _Requirements: 3.2, 15.1_

  - [x] 5.4 Implement `list_characters` tool
    - Return all characters (name, avatar, last modified) via `GET /api/characters/all`
    - _Requirements: 3.3_

  - [x] 5.5 Implement `read_persona`, `write_persona`, `list_personas` tools
    - Read/write/list user personas via ST REST API
    - Snapshot before write, send undo_available
    - _Requirements: 3.14, 3.15, 3.16_

  - [x] 5.6 Implement `read_world_entries` and `write_world_entry` tools
    - Read: retrieve world entries from local SQLite by ID or search query
    - Write: create or update world entry in SQLite, snapshot before update
    - Index new/updated entries in tantivy search index
    - Send undo_available after write
    - _Requirements: 3.4, 3.5, 15.1_

  - [x] 5.7 Implement `read_post_history` and `write_post_history` tools
    - Read/write post-history instructions (stored in SQLite config table or dedicated field)
    - Snapshot before write
    - _Requirements: 3.6, 3.7_

- [x] 6. Tool implementations — Search, Export, Preview, Project, Undo
  - [x] 6.1 Implement `search_wiki` tool
    - HTTP GET to fandom wiki search API (or configurable wiki URL)
    - Parse results, return structured summaries (title, snippet, URL)
    - _Requirements: 3.8_

  - [x] 6.2 Implement `search_local` tool (BM25 via tantivy)
    - Create `src/search/mod.rs` with tantivy index setup
    - Index world entries, character data, reference document chunks
    - Query by text, return top-N results with source attribution
    - _Requirements: 3.9_

  - [x] 6.3 Implement `export_card` tool
    - Assemble TavernCard V2 JSON structure from character data
    - Optionally embed JSON into PNG tEXt chunk (character card V3 spec)
    - Write output file to configurable export directory
    - _Requirements: 3.10_

  - [x] 6.4 Implement `show_preview` tool
    - Send a `preview` WebSocket event to the frontend with content_type (character, world, posthistory, persona) and data payload
    - No file I/O — this tool just relays data to the UI
    - _Requirements: 3.11_

  - [x] 6.5 Implement `create_project` and `manage_tasks` tools
    - `create_project`: insert into projects table (name, description, metadata JSON)
    - `manage_tasks`: CRUD operations on tasks table (create, update_status, list, delete) scoped to a project
    - _Requirements: 3.12, 3.13_

  - [x] 6.6 Implement `undo_change` and `list_versions` tools
    - `undo_change`: pop most recent snapshot from entity_versions, restore entity to that state (write back via ST API for characters, update SQLite for world entries)
    - `list_versions`: return version history for an entity (timestamp, summary)
    - Prune to 20 versions per entity
    - _Requirements: 15.2, 15.3, 15.4, 15.5_

  - [x] 6.7 Write unit tests for version store (snapshot, undo, prune)
    - Test snapshot creation on write
    - Test undo restores previous state
    - Test pruning keeps only last 20 versions
    - _Requirements: 15.1, 15.2, 15.3_

- [x] 7. Checkpoint — All tools compile and pass basic tests
  - Ensure all tests pass, ask the user if questions arise.

- [x] 8. WebSocket server
  - [x] 8.1 Implement WebSocket server with tokio-tungstenite
    - Create `src/ws/server.rs` — listen on configured port (default 7842)
    - Accept connections, spawn per-connection handler task
    - Parse incoming `ClientMessage` JSON (user_message, cancel, switch_model, new_conversation, undo, update_config)
    - _Requirements: 1.1, 9.1_

  - [x] 8.2 Implement streaming relay and event dispatch
    - Create `WebSocketSender` wrapper that serializes `ServerMessage` variants to JSON and sends via the WS connection
    - Events: token, thinking, message_complete, tool_start, tool_end, preview, error, status, undo_available, system_message, config_updated
    - Wire sender into agent loop so all events flow to the connected client
    - _Requirements: 2.1, 2.3, 1.3_

  - [x] 8.3 Implement cancel signal handling
    - On receiving `{ type: "cancel" }`, trigger the CancellationToken to abort the active agent turn
    - _Requirements: 8.2_

  - [x] 8.4 Implement model switching via WebSocket
    - On receiving `{ type: "switch_model", profile: "..." }`, update the active model profile on the LLM client
    - Send `config_updated` event back to confirm
    - _Requirements: 12.4_

  - [x] 8.5 Implement new conversation and undo commands
    - `new_conversation`: archive current conversation in SQLite, reset agent context, send system_message confirming
    - `undo`: call VersionStore.undo for the specified entity, send result back
    - _Requirements: 7.4, 15.3_

- [x] 9. HTTP API (axum)
  - [x] 9.1 Set up axum router and health endpoint
    - Create `src/http/mod.rs` with axum Router
    - `GET /health` — returns 200 with sidecar version and status
    - Mount on same port as WebSocket (or configurable separate port)
    - _Requirements: 9.4_

  - [x] 9.2 Implement config endpoints
    - `GET /config` — return current config (model profiles, post-card prompt, ST URL)
    - `PUT /config` — update config values, persist to SQLite
    - _Requirements: 13.3, 13.4_

  - [x] 9.3 Implement conversation history endpoint
    - `GET /conversations/:id` — return messages for a conversation
    - `GET /conversations` — list conversations (id, title, created_at)
    - _Requirements: 7.5_

  - [x] 9.4 Implement reference document endpoints
    - `POST /documents` — upload text/markdown document, chunk it, store in SQLite, index in tantivy
    - `GET /documents` — list uploaded documents (id, filename, size)
    - `DELETE /documents/:id` — remove document and its chunks from DB and index
    - Enforce limits: max 20 docs per project, 5MB combined text
    - _Requirements: 14.2, 14.3, 14.5, 14.6_

- [x] 10. Checkpoint — Sidecar fully functional end-to-end
  - Ensure all tests pass, ask the user if questions arise. Test manually: start sidecar, connect via WebSocket (e.g., websocat), send a user_message, verify streaming response and tool calls work.

- [x] 11. Svelte frontend scaffolding
  - [x] 11.1 Initialize Vite + Svelte project for ST extension
    - Create `frontend/` directory with `package.json`, `vite.config.ts`, `tsconfig.json`
    - Vite config: IIFE output format, single `dist/index.js` + `dist/index.css` bundle
    - Svelte plugin for `.svelte` compilation
    - CSS scoped to `.wb-root` container
    - _Requirements: 5.10, 10.1_

  - [x] 11.2 Create ST extension bootstrap (`main.ts`)
    - Register as SillyTavern extension (extension manifest, init function)
    - Create panel toggle button in ST UI
    - Mount Svelte `App.svelte` into the extension container
    - _Requirements: 5.1_

  - [x] 11.3 Create core Svelte stores
    - `connection.ts`: WebSocket connection state (connected, disconnected, reconnecting)
    - `conversation.ts`: messages array, streaming state, current streaming content
    - `ui.ts`: panel mode (chat-only vs split), active right-pane tab, preview data
    - `config.ts`: model profiles, active model, post-card prompt
    - _Requirements: 5.9, 11.4_

  - [x] 11.4 Implement WebSocket client (`ws/client.ts`)
    - Connect to sidecar on configurable port (default 7842)
    - Auto-reconnect with exponential backoff on disconnect
    - Parse incoming `ServerMessage` JSON and dispatch to stores
    - Send `ClientMessage` JSON (user_message, cancel, switch_model, new_conversation, undo)
    - _Requirements: 5.9, 11.4, 9.1_

- [x] 12. Chat UI components
  - [x] 12.1 Implement `PanelShell.svelte`
    - Fixed-position slide-out panel with resize handle (drag to resize width)
    - Open/close toggle button
    - Header with ENI branding and close button
    - Layout container that switches between full-width chat and split-pane mode
    - _Requirements: 5.1, 10.7_

  - [x] 12.2 Implement `ChatPane.svelte`
    - Scrollable message history area (auto-scroll on new messages)
    - Message input textarea with Send button (or Stop button when generating)
    - ENI status indicator in status bar (idle/thinking/executing tool)
    - Model selector dropdown in status bar
    - _Requirements: 5.2, 5.11, 8.1_

  - [x] 12.3 Implement `MessageBubble.svelte`
    - User messages: right-aligned, accent tint background
    - Assistant messages: left-aligned, surface background
    - No avatars — plain bubbles matching mockup style
    - Render markdown content (bold, italic, code blocks, lists)
    - Support incremental token rendering during streaming
    - _Requirements: 5.3, 2.2_

  - [x] 12.4 Implement `ThinkingBlock.svelte`
    - Collapsible block showing "Thinking..." label
    - Collapsed by default, user can expand to see reasoning content
    - Renders thinking content streamed from sidecar
    - _Requirements: 5.5_

  - [x] 12.5 Implement `ToolCallCard.svelte`
    - Compact inline card showing: tool name, brief description, status icon (spinner → ✓ or ✗)
    - Appears inline in the message flow when a tool is executed
    - _Requirements: 5.4_

  - [x] 12.6 Implement `ModelSelector.svelte`
    - Dropdown populated from config store (model profiles)
    - On selection change, send `switch_model` message via WebSocket
    - Display active model name
    - _Requirements: 5.11, 12.3_

  - [x] 12.7 Implement `UndoToast.svelte`
    - Toast notification at bottom of panel after write operations
    - Shows entity name and "Undo" button
    - On click, sends `undo` message via WebSocket
    - Auto-dismisses after 10 seconds
    - _Requirements: 6.9, 15.4_

- [x] 13. Right pane (tabbed preview panel)
  - [x] 13.1 Implement `RightPane.svelte` container
    - Tabbed container with tabs: Character, World, Post-History, Persona, Settings
    - Close button (returns to full-width chat)
    - Escape key closes pane
    - Opens automatically when `preview` event received, switches to appropriate tab
    - _Requirements: 5.6, 5.7, 5.8, 6.6_

  - [x] 13.2 Implement `CharacterTab.svelte`
    - Avatar with initial letter, character name, tags as pill badges
    - Sections: Description, Personality, Scenario, First Message (monospace)
    - Copy-to-clipboard button
    - _Requirements: 6.1, 6.7_

  - [x] 13.3 Implement `WorldTab.svelte`
    - List of world entries with title, keyword tags (pill badges), content text
    - Scrollable list
    - Copy button per entry
    - _Requirements: 6.2, 6.7_

  - [x] 13.4 Implement `PostHistoryTab.svelte`
    - Narration style rules, formatting rules in monospace
    - Tone keywords as pill badges
    - Copy button
    - _Requirements: 6.3, 6.7_

  - [x] 13.5 Implement `PersonaTab.svelte`
    - Avatar with initial, name, tags
    - Persona description text
    - Relationship to active character
    - Copy button
    - _Requirements: 6.4, 6.7_

  - [x] 13.6 Implement `SettingsTab.svelte`
    - Model profile configuration form (base URL, API key, model, temperature, max tokens)
    - Post-card prompt textarea (editable, saves on blur)
    - Reference documents list with upload button and remove button per doc
    - Sidecar connection status indicator
    - _Requirements: 6.5, 13.2, 14.1, 14.5_

- [x] 14. Checkpoint — Frontend renders and connects to sidecar
  - Ensure all tests pass, ask the user if questions arise. Commit all uncommited changes and push to main. Tell the user instructions on how to install the extension into ST, and how to run it. Build the frontend (`npm run build`), The user will connect it in ST and verify the websocket connets, send a message, see streaming response.

- [x] 15. Integration and wiring
  - [x] 15.1 Wire preview events to right pane
    - When sidecar sends `preview` event, update ui store, open right pane, switch to correct tab, render data
    - When `show_preview` is called multiple times, update in place
    - _Requirements: 6.6, 6.8_

  - [x] 15.2 Wire undo flow end-to-end
    - Sidecar sends `undo_available` → frontend shows UndoToast → user clicks Undo → frontend sends `undo` WS message → sidecar restores version → sends confirmation
    - _Requirements: 15.3, 15.4_

  - [x] 15.3 Wire model selector end-to-end
    - Frontend loads model profiles from sidecar config → populates dropdown → user selects → sends `switch_model` → sidecar confirms → status bar updates
    - _Requirements: 12.3, 12.4, 12.5_

  - [x] 15.4 Wire cancellation end-to-end
    - Send button becomes Stop button during generation → user clicks Stop → sends `cancel` → sidecar aborts → sends "Generation stopped" → frontend returns to idle
    - _Requirements: 8.1, 8.2, 8.3_

  - [x] 15.5 Wire reconnection and error display
    - On WebSocket disconnect: show reconnection indicator, attempt reconnect with exponential backoff
    - On sidecar errors: display error messages in chat as system messages
    - On sidecar not running: show setup guide
    - _Requirements: 11.4, 9.2_

  - [x] 15.6 Wire conversation persistence and restore
    - On panel open: fetch conversation history from sidecar HTTP API, populate message list
    - New Chat button: send `new_conversation`, clear messages, show welcome
    - _Requirements: 7.2, 7.4, 7.5_

- [x] 16. ENI system prompt crafting
  - [x] 16.1 Write ENI's base personality system prompt
    - Create `prompts/eni-system.md` (or embed in Rust as a const)
    - Define ENI's personality: helpful creative writing assistant, conversational, world-building focused
    - Include tool usage instructions: when to use each tool, how to present results
    - Include formatting guidelines: markdown usage, preview triggering, undo notification
    - Reference the interaction flows from the design document
    - _Requirements: 4.1, 13.5_

  - [x] 16.2 Integrate system prompt into context builder
    - Load ENI system prompt at sidecar startup
    - Concatenate with post-card prompt (if set) in context builder
    - Ensure tool definitions are included in the standard format
    - _Requirements: 4.1, 13.1_

- [ ] 17. Testing and dev tooling
  - [ ] 17.1 Write integration tests for the agent loop
    - Mock LLM API (return canned SSE responses with tool calls)
    - Verify: tool dispatch, iteration limiting, cancellation, conversation persistence
    - _Requirements: 1.1, 1.2, 1.5_

  - [ ] 17.2 Write integration tests for WebSocket protocol
    - Connect test client, send user_message, verify event sequence (status → token → message_complete)
    - Test cancel flow, model switch flow
    - _Requirements: 2.1, 8.2, 12.4_

  - [ ] 17.3 Write integration tests for ST API client
    - Mock ST REST API responses
    - Verify character CRUD, CSRF handling, error cases
    - _Requirements: 3.1, 3.2, 3.3, 11.3_

  - [ ] 17.4 Set up dev tooling
    - Create `Makefile` or `justfile` with commands: build, run, test, fmt, clippy
    - Create `docker-compose.yml` or script for running sidecar + ST together in dev
    - Add `.cargo/config.toml` for fast compile settings (incremental, codegen-units)
    - Document dev setup in `README.md`
    - _Requirements: 9.5_

- [x] 18. CI pipeline — Rust sidecar release builds + extension auto-download
  - [x] 18.1 Create GitHub Actions workflow for sidecar cross-compilation
    - Create `.github/workflows/release-sidecar.yml`
    - Build matrix: linux-x64, darwin-x64, darwin-arm64, win32-x64
    - Trigger on version tags (`v*`) — only compiles the Rust sidecar, not the frontend
    - Use `dtolnay/rust-toolchain@stable` for Rust setup
    - Build release binaries with `cargo build --release --target <triple>` in `eni-sidecar/`
    - Name artifacts clearly: `eni-sidecar-<platform>-<arch>` (e.g., `eni-sidecar-darwin-arm64`)
    - Upload each binary as a build artifact
    - _Requirements: 9.2_

  - [x] 18.2 Create GitHub Release job
    - Add `release` job that depends on all build jobs
    - Download all artifacts and attach to a GitHub Release using `softprops/action-gh-release@v2`
    - Include release notes template (changelog or auto-generated)
    - Tag format: `v0.1.0`, `v0.2.0`, etc.
    - _Requirements: 9.2_

  - [x] 18.3 Implement binary auto-download in ST server plugin (`index.js`)
    - On extension load, check if `bin/eni-sidecar` (platform-appropriate name) exists locally
    - If binary is present and healthy (version check via `--version` flag), spawn it normally
    - If binary is missing or outdated:
      - Query GitHub Releases API (`/repos/{owner}/{repo}/releases/latest`) for the latest release
      - Identify the correct asset by matching `process.platform` + `process.arch` to the artifact name
      - Download the binary to `bin/` directory within the extension folder
      - Set executable permissions on unix platforms (`chmod 0o755`)
      - Log download progress to ST logger
    - After download completes, spawn the sidecar as normal
    - Handle errors gracefully:
      - No internet: log warning, show message in frontend that sidecar is unavailable
      - Rate-limited by GitHub API: retry with backoff, fall back to manual download message
      - Unsupported platform: surface clear error with platform info
      - Corrupt/partial download: delete partial file, retry once
    - _Requirements: 9.2, 9.4, 11.1_

  - [x] 18.4 Add first-run / download-failed UX in frontend
    - If sidecar binary is missing and auto-download fails, show a message in the chat panel explaining the situation
    - Provide a direct link to the GitHub Releases page for manual download
    - Show detected platform/arch so the user knows which binary to grab
    - Once binary is manually placed, user can reload the extension to trigger spawn
    - _Requirements: 9.11_

- [ ] 19. Final checkpoint — Full system operational
  - Ensure all tests pass, ask the user if questions arise. Verify end-to-end: start sidecar, open ST with extension, send messages, see streaming, tool calls execute, preview pane works, undo works, model switching works. Verify CI: push a tag, confirm binaries are built and attached to the release.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- The sidecar is built first (tasks 1–10) because the frontend depends on it
- Reference repos should be consulted during implementation for patterns: `reference-repos/openai-codex/codex-rs/core/` for Rust architecture, `reference-repos/building-an-agent/` for the minimal loop pattern, `reference-repos/agent-loop/` for iteration limits and guards
- The UI mockup at `mockups/eni-ui-mockup.html` is the canonical visual reference for all frontend components
