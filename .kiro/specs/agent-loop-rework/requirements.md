# Requirements Document

## Introduction

Rework the SillyTavern World Builder extension from its current over-engineered React multi-view architecture into a clean Codex-style agent loop with a Rust sidecar backend and Svelte frontend. ENI (the AI agent) becomes the sole primary interface — always conversational, chat-first.

**IMPORTANT: Always refer to the UI mockup at `mockups/eni-ui-mockup.html` (with `mockups/styles.css` and `mockups/mockup.js`) as the canonical reference for how the UI should look and behave. Open it in a browser to see the interactive prototype.**

**Architecture:**
- **Svelte frontend** (ST extension sidebar panel) — thin client that renders chat, sends messages to the sidecar, displays streamed responses and previews
- **Rust sidecar** (local HTTP + WebSocket server) — runs the agent loop, executes tools, manages context, talks directly to OpenAI-compatible LLM APIs with proper SSE streaming, reads/writes ST data via ST's REST API
- **Node.js server plugin** (ST extension `index.js`) — manages the sidecar binary lifecycle: spawns on extension load, monitors health, restarts on crash, kills on shutdown. Ships platform-specific binaries so the user never manually starts the sidecar.

The custom prompt assembly system, regex-based tool call parser, project/task board, and tab-based ViewRouter are replaced by a tight agent loop running in Rust that calls LLM APIs directly (not through ST's `generateRaw`) for true streaming support.

What stays: Data models (characters, world entries, post-history), the theme/styling system concepts, and the panel shell behavior (resize, open/close). The persistence layer moves from browser IndexedDB to the Rust sidecar (SQLite or similar).

**Reference implementations** (cloned in `reference-repos/`):
- `reference-repos/openai-codex/codex-rs/core/` — Rust agent loop with tools, context management, streaming, sandboxing
- `reference-repos/building-an-agent/` — Minimal ~200 line TypeScript agent showing the core LLM + tools + loop pattern
- `reference-repos/agent-loop/` — Python agent with completion detection, repetition detection, iteration limits

## Glossary

- **ENI**: The AI agent embedded in the World Builder extension; the primary user interface
- **Agent_Loop**: The core execution cycle running in the Rust sidecar: user message → build context → call LLM API → parse structured response → execute tools → stream results back → repeat until done
- **Rust_Sidecar**: A local HTTP + WebSocket server (written in Rust) that runs the agent loop, executes tools, manages context, and communicates with LLM APIs directly. Spawned and managed automatically by the ST server-side plugin.
- **Server_Plugin**: The Node.js `index.js` file in the ST extension that runs in SillyTavern's server process. Responsible for spawning, health-checking, restarting, and killing the Rust sidecar binary.
- **Svelte_Frontend**: The SillyTavern extension UI built with Svelte; a thin client that renders chat, sends messages to the sidecar via WebSocket, and displays streamed responses
- **Tool_Call**: A structured JSON object returned by the LLM describing a tool invocation with name and arguments
- **Tool_Result**: The structured response from executing a tool, containing success status and data or error
- **Preview_Pane**: An optional side panel that ENI can populate with rendered content (character cards, world entries, markdown previews)
- **Conversation_Store**: Conversation history managed by the Rust sidecar, persisted to local SQLite
- **World_Document_Store**: World entries (lore book nodes) managed by the Rust sidecar, persisted to local SQLite
- **Panel_Shell**: The fixed-position slide-out panel container in the Svelte frontend with resize handle and open/close toggle
- **Context_Builder**: The Rust module responsible for assembling the LLM prompt from system prompt, conversation history, and tool definitions
- **SSE_Streaming**: Server-Sent Events streaming from the LLM API, relayed to the Svelte frontend via WebSocket for real-time token display
- **ST_REST_API**: SillyTavern's HTTP API used by the Rust sidecar to read/write character data and world information

## Requirements

### Requirement 1: Agent Loop Core (Rust Sidecar)

**User Story:** As a user, I want ENI to process my messages through a reliable agent loop running in a local Rust server, so that responses stream in real-time and tool execution doesn't block the UI.

#### Acceptance Criteria

1. WHEN a user message is received via WebSocket, THE Rust_Sidecar SHALL build a context payload containing the system prompt, conversation history, and tool definitions, then invoke the configured LLM API
2. WHEN the LLM API returns a response containing a Tool_Call, THE Rust_Sidecar SHALL execute the tool, append the Tool_Result to the context, and invoke the LLM API again
3. WHILE the Agent_Loop is executing tool calls, THE Rust_Sidecar SHALL stream status updates to the Svelte_Frontend via WebSocket (e.g., "executing tool: read_character")
4. WHEN the LLM returns a text response with no Tool_Call, THE Rust_Sidecar SHALL stream the response tokens to the Svelte_Frontend and append the final message to the Conversation_Store
5. IF the Agent_Loop exceeds 15 iterations without producing a final response, THEN THE Rust_Sidecar SHALL halt execution, send an error message to the frontend, and return to idle
6. IF the LLM API throws an error or times out, THEN THE Rust_Sidecar SHALL send a user-friendly error message to the frontend and allow retry
7. THE Rust_Sidecar SHALL support any OpenAI-compatible API endpoint (configurable base URL and API key)

### Requirement 2: Real-Time Streaming

**User Story:** As a user, I want to see ENI's response tokens appear in real-time as they're generated, so that the interface feels responsive and I can read along as ENI thinks.

#### Acceptance Criteria

1. WHEN the LLM API streams response tokens via SSE, THE Rust_Sidecar SHALL relay each token to the Svelte_Frontend via WebSocket with minimal latency (<50ms per token relay)
2. THE Svelte_Frontend SHALL render streamed tokens incrementally in the chat message area as they arrive
3. WHEN a tool call is detected mid-stream, THE Rust_Sidecar SHALL pause token display, show a tool execution indicator, then resume streaming after the tool result is processed
4. THE Svelte_Frontend SHALL display a typing indicator while waiting for the first token from the sidecar

### Requirement 3: Tool Definitions

**User Story:** As a user, I want ENI to have a comprehensive set of tools for managing characters, world entries, post-history, projects, searching, exporting, and previewing, so that ENI can do everything conversationally without needing separate UI views.

#### Acceptance Criteria

1. THE Rust_Sidecar SHALL expose a `read_character` tool that reads one or more fields from the active SillyTavern character card via ST_REST_API
2. THE Rust_Sidecar SHALL expose a `write_character` tool that writes one or more fields to the active SillyTavern character card via ST_REST_API
3. THE Rust_Sidecar SHALL expose a `list_characters` tool that returns all characters available in SillyTavern (name, avatar, last modified) via ST_REST_API
4. THE Rust_Sidecar SHALL expose a `read_world_entries` tool that retrieves world/lore book entries from the World_Document_Store by ID or search query
5. THE Rust_Sidecar SHALL expose a `write_world_entry` tool that creates or updates a world/lore book entry in the World_Document_Store
6. THE Rust_Sidecar SHALL expose a `read_post_history` tool that retrieves the current post-history instructions
7. THE Rust_Sidecar SHALL expose a `write_post_history` tool that updates the post-history instructions
8. THE Rust_Sidecar SHALL expose a `search_wiki` tool that queries a fandom wiki and returns structured results
9. THE Rust_Sidecar SHALL expose a `search_local` tool that performs vector similarity search across all local world entries and character data, returning the most relevant matches
10. THE Rust_Sidecar SHALL expose an `export_card` tool that assembles and exports a TavernCard V2 PNG or JSON file
11. THE Rust_Sidecar SHALL expose a `show_preview` tool that sends rendered content to the Svelte_Frontend for display in the Preview_Pane
12. THE Rust_Sidecar SHALL expose a `create_project` tool that creates a new world-building project with a name, description, and optional metadata (genre, setting, tone)
13. THE Rust_Sidecar SHALL expose a `manage_tasks` tool that creates, updates, lists, or completes tasks within a project (actions: create, update_status, list, delete)
14. THE Rust_Sidecar SHALL expose a `read_persona` tool that reads the active user persona from SillyTavern via ST_REST_API
15. THE Rust_Sidecar SHALL expose a `write_persona` tool that updates the active user persona in SillyTavern via ST_REST_API
16. THE Rust_Sidecar SHALL expose a `list_personas` tool that returns all available user personas in SillyTavern via ST_REST_API
17. WHEN a tool is invoked, THE Rust_Sidecar SHALL validate the arguments against the tool's parameter schema before execution
18. IF tool argument validation fails, THEN THE Rust_Sidecar SHALL return a Tool_Result with success=false and a descriptive validation error

### Requirement 4: Context Builder (Rust)

**User Story:** As a developer, I want a fast context builder in Rust that assembles the LLM prompt from a fixed system prompt and conversation history, so that prompt construction is efficient and doesn't add latency.

#### Acceptance Criteria

1. THE Context_Builder SHALL assemble the prompt as chat-completion messages: system message (ENI personality + tool definitions) followed by conversation history (user, assistant, tool messages)
2. THE Context_Builder SHALL format messages using the OpenAI chat-completion message format (role + content) for compatibility with any OpenAI-compatible API
3. WHEN the total token count of the assembled context exceeds the configured maximum, THE Context_Builder SHALL truncate oldest messages from the conversation history while preserving the system prompt and the most recent 4 messages
4. THE Context_Builder SHALL use a fast token counter (tiktoken-rs or similar) for accurate token estimation
5. THE Context_Builder SHALL include tool definitions in the standard OpenAI function-calling format (tools array with JSON schemas)

### Requirement 5: UI — Svelte Chat-First Layout

**User Story:** As a user, I want the panel to show ENI chat at full width by default with no tabs or navigation, so that the conversational interface is the primary and only required interaction mode.

**IMPORTANT: Refer to `mockups/eni-ui-mockup.html` for the canonical visual reference.**

#### Acceptance Criteria

1. WHEN the Panel_Shell is opened, THE Svelte_Frontend SHALL display the ENI chat interface at full panel width with no tab bar, no view navigation, and no project board
2. THE Svelte_Frontend SHALL display a message input area with send button, an ENI status indicator (idle/thinking/executing tool), and a scrollable message history
3. Messages SHALL be rendered as plain bubbles without avatars — user messages right-aligned with accent tint, assistant messages left-aligned with surface background (matching the mockup style)
4. Tool calls SHALL be rendered as compact inline cards showing tool name, description, and status (✓ on success)
5. WHEN ENI is thinking, THE Svelte_Frontend SHALL display a collapsible "Thinking..." block (collapsed by default) that the user can expand to see ENI's reasoning process
6. WHEN ENI invokes the show_preview tool, THE Svelte_Frontend SHALL split into a two-pane layout: chat on the left (40% width) and a tabbed right pane (60% width)
7. THE right pane SHALL have tabs for: Character, World, Post-History, Persona, and Settings
8. WHEN the user closes the right pane (via close button or Escape key), THE Svelte_Frontend SHALL return to full-width chat layout
9. THE Svelte_Frontend SHALL connect to the Rust_Sidecar via WebSocket on panel open and reconnect automatically on disconnect
10. THE Svelte_Frontend SHALL be built with Svelte (compiled, no virtual DOM) for minimal bundle size and fast DOM updates during streaming
11. THE Svelte_Frontend SHALL include a model selector dropdown in the ENI status bar for switching between configured model profiles

### Requirement 6: UI — Preview Pane (Tabbed Right Panel)

**User Story:** As a user, I want ENI to be able to show me previews of characters, world entries, post-history blocks, and personas in a tabbed panel, so that I can review all content types visually.

**IMPORTANT: Refer to `mockups/eni-ui-mockup.html` for the canonical visual reference of each tab.**

#### Acceptance Criteria

1. THE right pane SHALL render a **Character** tab showing: avatar with initial, name, tags, and sections for Description, Personality, Scenario, and First Message (first message in monospace)
2. THE right pane SHALL render a **World** tab showing: a list of world entries, each with title, keyword tags (pill badges), and content text
3. THE right pane SHALL render a **Post-History** tab showing: narration style rules, formatting rules, and tone keywords (as pill badges) — all in monospace for the rule content
4. THE right pane SHALL render a **Persona** tab showing: avatar with initial, name, tags, persona description, and relationship to the active character
5. THE right pane SHALL render a **Settings** tab showing: model profile config (base URL, API key, model, temperature, max tokens), post-card prompt textarea, reference documents list with upload/remove, and sidecar connection status
6. WHEN the show_preview tool is invoked, THE right pane SHALL open (if not already open) and switch to the appropriate tab based on the content_type
7. THE right pane SHALL include a copy-to-clipboard button for rendered content
8. THE right pane SHALL update in place if ENI invokes show_preview again while the pane is already open
9. AFTER ENI performs a write operation, THE Svelte_Frontend SHALL display an undo toast at the bottom of the panel with entity name and an "Undo" button

### Requirement 7: Persistence (Rust Sidecar — SQLite)

**User Story:** As a user, I want my conversation with ENI and my world data to persist across sessions, so that I don't lose context when I close and reopen the panel or restart SillyTavern.

#### Acceptance Criteria

1. THE Rust_Sidecar SHALL persist conversations, world entries, and configuration to a local SQLite database
2. WHEN the Rust_Sidecar starts, IT SHALL load the most recent conversation and restore state
3. THE Rust_Sidecar SHALL store messages with role, content, timestamp, and optional tool call/result metadata
4. WHEN the user starts a new conversation (via a "New Chat" action from the frontend), THE Rust_Sidecar SHALL archive the current conversation and start fresh
5. THE Rust_Sidecar SHALL expose an HTTP endpoint for the Svelte_Frontend to retrieve conversation history on connect

### Requirement 8: Cancellation and Abort

**User Story:** As a user, I want to be able to stop ENI mid-generation, so that I can interrupt long-running or unwanted responses.

#### Acceptance Criteria

1. WHILE the Agent_Loop is executing, THE Svelte_Frontend SHALL display a "Stop" button in place of the "Send" button
2. WHEN the user clicks "Stop", THE Svelte_Frontend SHALL send a cancel message via WebSocket, and THE Rust_Sidecar SHALL abort the current LLM API call and halt any pending tool execution
3. WHEN generation is aborted, THE Rust_Sidecar SHALL send a "Generation stopped" system message to the frontend and return to idle

### Requirement 9: Sidecar Lifecycle (Auto-Managed)

**User Story:** As a user, I want the Rust sidecar to start and stop automatically with the extension, so that I never need to manage a separate process manually.

#### Acceptance Criteria

1. THE Extension SHALL include a server-side plugin (`index.js`) that runs in SillyTavern's Node.js process and manages the sidecar binary lifecycle
2. THE server-side plugin SHALL ship platform-specific sidecar binaries in a `bin/` directory (darwin-arm64, darwin-x64, linux-x64, win-x64) and select the correct one at runtime based on `process.platform` and `process.arch`
3. WHEN the ST extension loads, THE server-side plugin SHALL spawn the Rust_Sidecar as a child process on a configurable local port (default: 7842), piping stdout/stderr to ST's logger
4. IF the configured port is already in use (sidecar already running from a previous session or manual start), THE server-side plugin SHALL skip spawning and reuse the existing process
5. WHEN SillyTavern shuts down or the extension is unloaded, THE server-side plugin SHALL send SIGTERM to the sidecar child process and wait up to 5 seconds before force-killing it
6. THE Svelte_Frontend SHALL connect to the Rust_Sidecar via WebSocket on panel open, using the port provided by the server-side plugin
7. IF the sidecar process crashes, THE server-side plugin SHALL attempt to restart it once automatically, then surface an error to the frontend if the restart also fails
8. THE Rust_Sidecar SHALL accept configuration via a TOML or JSON config file specifying: LLM API base URL, API key, model name, max context tokens, and ST base URL
9. THE Rust_Sidecar SHALL provide a health-check HTTP endpoint (`GET /health`) that the server-side plugin polls after spawn to confirm readiness before notifying the frontend
10. THE Rust_Sidecar SHALL log startup, connections, and errors to stdout for debugging (captured by the server-side plugin and forwarded to ST's log)
11. THE Extension SHALL support a first-run setup flow: if no sidecar binary is found for the current platform, THE Svelte_Frontend SHALL display a message indicating the platform is unsupported or that the binary needs to be downloaded

### Requirement 10: Remove Legacy Architecture

**User Story:** As a developer, I want the old over-engineered systems removed, so that the codebase is simple and maintainable.

#### Acceptance Criteria

1. THE Extension SHALL NOT use React, Zustand, or the existing webpack build system (replaced by Svelte + Vite or similar)
2. THE Extension SHALL NOT use the existing layered prompt assembly system, regex-based tool call parsing, or browser-side agent loop
3. THE Extension SHALL NOT render the ViewRouter component, tab navigation, or ProjectBoardView as user-facing navigation
4. THE Extension SHALL NOT use the Project or Task data models for workflow management (no project-store, no task selection, no task dependencies, no component vocabulary system)
5. THE Extension SHALL NOT use the schema-generator service or COMPONENT_VOCABULARIES mapping
6. THE Extension SHALL NOT use browser-side IndexedDB for persistence (moved to Rust sidecar SQLite)
7. THE Extension SHALL retain the panel shell behavior (slide-out panel, resize handle, open/close toggle) reimplemented in Svelte
8. THE Extension SHALL retain the visual design language (colors, fonts, spacing tokens) from the existing theme system

### Requirement 11: Error Handling

**User Story:** As a user, I want clear error messages when something goes wrong, so that I understand what happened and can take action.

#### Acceptance Criteria

1. IF the Rust_Sidecar cannot connect to the configured LLM API, THEN IT SHALL send an error message to the frontend indicating the API configuration issue
2. IF a tool execution fails, THEN THE Rust_Sidecar SHALL include the error in the Tool_Result fed back to the LLM so ENI can explain the failure conversationally
3. IF the ST_REST_API is unavailable when character/world tools are invoked, THEN THE Rust_Sidecar SHALL return a Tool_Result indicating SillyTavern is not reachable
4. IF the WebSocket connection between frontend and sidecar drops, THE Svelte_Frontend SHALL display a reconnection indicator and attempt to reconnect with exponential backoff
5. WHEN an unrecoverable error occurs during the agent loop, THE Rust_Sidecar SHALL send the error to the frontend as a system message with context for retry

### Requirement 12: Multi-Model Selection

**User Story:** As a user, I want to be able to choose which LLM model ENI uses and switch between models mid-session, so that I can use a fast cheap model for quick tasks and a powerful model for creative writing.

#### Acceptance Criteria

1. THE Rust_Sidecar SHALL support configuring multiple named model profiles, each with a base URL, API key, model name, and optional parameters (temperature, max tokens)
2. THE Rust_Sidecar SHALL have a default model profile that is used when no explicit selection is made
3. THE Svelte_Frontend SHALL provide a model selector (dropdown or similar) that allows the user to switch the active model at any time
4. WHEN the user switches models mid-conversation, THE Rust_Sidecar SHALL use the newly selected model for all subsequent LLM calls without restarting the conversation
5. THE Rust_Sidecar SHALL include the active model name in status messages sent to the frontend so the user knows which model is responding

### Requirement 13: System Prompt Customization

**User Story:** As a user, I want to customize ENI's behavior with a user-editable system prompt that comes after ENI's base personality card, so that I can adjust writing style, tone, or domain focus without modifying the extension code.

#### Acceptance Criteria

1. THE Rust_Sidecar SHALL support a user-editable "post-card" system prompt that is appended after ENI's fixed personality prompt in the context
2. THE Svelte_Frontend SHALL provide a settings interface where the user can view and edit the post-card system prompt
3. WHEN the post-card prompt is modified, THE Rust_Sidecar SHALL use the updated prompt for all subsequent LLM calls in the current and future conversations
4. THE Rust_Sidecar SHALL persist the post-card prompt to the SQLite database so it survives restarts
5. THE post-card prompt SHALL be optional — if empty or not set, THE Rust_Sidecar SHALL use only ENI's base personality prompt

### Requirement 14: Context Injection / Reference Documents

**User Story:** As a user, I want to upload or paste reference documents (campaign notes, character backstories, wiki pages) that ENI can access as persistent context, so that ENI has background knowledge available across the entire conversation.

#### Acceptance Criteria

1. THE Svelte_Frontend SHALL allow the user to upload or paste text documents (plain text, markdown) as reference material
2. THE Rust_Sidecar SHALL store uploaded reference documents in the SQLite database, associated with the current project
3. THE Rust_Sidecar SHALL chunk and index reference documents for retrieval (using the same vector/similarity search as `search_local`)
4. WHEN building context for an LLM call, THE Context_Builder SHALL include relevant chunks from reference documents when they are semantically related to the current conversation (retrieved via the search index)
5. THE Svelte_Frontend SHALL display a list of uploaded reference documents with the ability to remove them
6. THE Rust_Sidecar SHALL support a maximum of 20 reference documents per project, with a combined size limit of 5MB of text

### Requirement 15: Undo / Version History

**User Story:** As a user, I want to be able to revert changes that ENI makes to character cards and world entries, so that I can experiment freely and roll back if I don't like the result.

#### Acceptance Criteria

1. WHEN the `write_character` or `write_world_entry` tool modifies an entity, THE Rust_Sidecar SHALL save a versioned snapshot of the entity's previous state before applying the change
2. THE Rust_Sidecar SHALL retain the last 20 versions per entity (character or world entry)
3. THE Rust_Sidecar SHALL expose an `undo_change` tool that reverts the most recent change to a specified entity, restoring the previous version
4. THE Svelte_Frontend SHALL display an undo indicator (toast or inline) after ENI makes a write, allowing the user to trigger undo with one click
5. THE Rust_Sidecar SHALL expose a `list_versions` tool that returns the version history for a specified entity (timestamp, summary of what changed)

