# Design Document: Agent Loop Rework

## Overview

This document describes the technical design for the ENI World Builder — a Rust sidecar + Svelte frontend architecture that replaces the previous React-based SillyTavern extension. ENI is an agentic AI assistant for building character cards, world lore, and post-history instructions through natural conversation.

**IMPORTANT: The UI mockup at `mockups/eni-ui-mockup.html` is the canonical visual reference. Always open it in a browser when implementing frontend components. It demonstrates the exact layout, styling, interactions, and content presentation for all views.**

The system follows the Codex-style agent loop pattern: a tight loop of `user message → context assembly → LLM call → tool execution → stream response` running in a local Rust server, with a thin Svelte client rendering the chat UI inside SillyTavern's sidebar. The sidecar binary is auto-managed by a Node.js server plugin that spawns it on extension load and kills it on shutdown — the user never interacts with it directly.

**Reference implementations:**
- `reference-repos/openai-codex/codex-rs/core/` — Rust agent loop architecture (agent, tools, context, streaming)
- `reference-repos/building-an-agent/` — Minimal TypeScript agent pattern (LLM + tools + loop)
- `reference-repos/agent-loop/` — Completion detection, repetition detection, iteration limits

---

## User Interaction Flow

### First-time setup
1. User installs the ST extension (includes Svelte frontend + server plugin + platform-specific sidecar binaries)
2. User configures LLM API endpoint, API key, and model name via the Settings tab in the panel (or edits the TOML config file directly)
3. User opens ST, clicks the World Builder panel toggle → server plugin auto-spawns the sidecar, panel slides open, connects via WebSocket

### Typical session — building a character from scratch
1. Panel opens → full-width ENI chat, welcome message: "Hey, I'm ENI. What are we building today?"
2. User: "I want to create a cyberpunk street doc character named Tomás"
3. ENI: "Cool. Let me check what you've got so far." → *calls `list_characters`* → "I see you don't have a Tomás yet. Want me to start from scratch, or base it on an existing character?"
4. User: "From scratch. He's a paranoid underground medic in Sector 7."
5. ENI starts drafting → *calls `write_character` with description, personality, scenario* → *calls `show_preview` to display the card*
6. Preview pane slides open on the right showing the character card summary
7. User: "The personality is too generic. Make him more twitchy and reference his military background."
8. ENI: "Got it." → *calls `write_character` to update personality* → preview updates in place
9. User sees the undo toast: "Character updated. [Undo]"

### Research-assisted world building
1. User: "I need lore for the corporate hierarchy. Search the Cyberpunk wiki for megacorp structures."
2. ENI → *calls `search_wiki` with query "cyberpunk megacorporation hierarchy"* → gets results
3. ENI: "Here's what I found..." → summarizes, then: "Want me to create world entries based on this?"
4. User: "Yeah, create entries for the top 3 corps"
5. ENI → *calls `write_world_entry` three times* → *calls `show_preview` with a summary*

### Reference document workflow
1. User uploads a PDF of their campaign notes via the settings panel
2. ENI now has that context available — when the user asks "What did I write about the Undercity?", ENI → *calls `search_local`* → finds relevant chunks from the uploaded doc and responds

### Project management (lightweight)
1. User: "Let's organize this. Create a project called 'Neon Veins' and break it into tasks."
2. ENI → *calls `create_project`* → "Done. What tasks do you want?"
3. User: "Build 3 characters, write district lore, set up post-history narration style"
4. ENI → *calls `manage_tasks` to create each task* → "All set. Want to start with Tomás?"

### Model switching
1. User is drafting quick lore entries → uses a fast model (e.g., GPT-4o-mini)
2. User: "Switch to Claude for the character personality — I want better creative writing"
3. User clicks model selector dropdown → switches to Claude profile
4. ENI continues the conversation using Claude for subsequent responses

---

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  SillyTavern Browser                                        │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Svelte Frontend (ST Extension - sidebar panel)       │  │
│  │  ┌─────────────────┐  ┌────────────────────────────┐ │  │
│  │  │  Chat Pane      │  │  Preview Pane (optional)   │ │  │
│  │  │  - Messages     │  │  - Markdown render         │ │  │
│  │  │  - Input        │  │  - Character card view     │ │  │
│  │  │  - Status       │  │  - World entry view        │ │  │
│  │  │  - Model select │  │  - Copy button             │ │  │
│  │  └─────────────────┘  └────────────────────────────┘ │  │
│  └──────────────────────────┬────────────────────────────┘  │
│                             │ WebSocket (port 7842)          │
└─────────────────────────────┼───────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│  SillyTavern Server (Node.js)                               │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Server Plugin (index.js — runs in ST's Node process)  │ │
│  │  - Spawns sidecar binary on extension load             │ │
│  │  - Selects platform-specific binary (bin/ directory)   │ │
│  │  - Health-checks sidecar (GET /health polling)         │ │
│  │  - Restarts on crash (1 retry, then error to frontend) │ │
│  │  - Kills sidecar on ST shutdown (SIGTERM → SIGKILL)    │ │
│  │  - Pipes sidecar stdout/stderr to ST logger            │ │
│  └────────────────────────────────────────────────────────┘ │
│  - Character CRUD endpoints                                 │
│  - World/Lorebook endpoints                                 │
│  - Chat history                                             │
└──────────────────────────────┬──────────────────────────────┘
                               │ child_process.spawn()
                               ▼
┌─────────────────────────────────────────────────────────────┐
│  Rust Sidecar (eni-sidecar)                                 │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  WebSocket Server (tokio + tungstenite)                │ │
│  │  - Receives user messages                              │ │
│  │  - Streams tokens + status updates to frontend         │ │
│  │  - Handles cancel signals                              │ │
│  └────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Agent Loop                                            │ │
│  │  - Context Builder (system prompt + history + tools)   │ │
│  │  - LLM Client (OpenAI-compatible, SSE streaming)       │ │
│  │  - Tool Dispatcher (validate args → execute → result)  │ │
│  │  - Iteration limiter (max 15 turns)                    │ │
│  │  - Repetition detector                                 │ │
│  └────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Tool Implementations                                  │ │
│  │  - Character tools (read/write/list via ST REST API)   │ │
│  │  - Persona tools (read/write/list via ST REST API)     │ │
│  │  - World tools (read/write via local SQLite)           │ │
│  │  - Post-history tools (read/write)                     │ │
│  │  - Search tools (wiki HTTP, local vector/BM25)         │ │
│  │  - Export tool (TavernCard V2 assembly)                 │ │
│  │  - Preview tool (sends render payload to frontend)     │ │
│  │  - Project/task tools (CRUD via SQLite)                │ │
│  │  - Version tools (undo, list_versions)                 │ │
│  └────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Persistence (SQLite via rusqlite)                     │ │
│  │  - conversations, messages                             │ │
│  │  - world_entries, characters (cached)                  │ │
│  │  - projects, tasks                                     │ │
│  │  - reference_documents, chunks                         │ │
│  │  - entity_versions (snapshots for undo)                │ │
│  │  - config (model profiles, post-card prompt)           │ │
│  └────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Search Index                                          │ │
│  │  - BM25 full-text index (tantivy)                      │ │
│  │  - Optional: vector embeddings (fastembed-rs)          │ │
│  │  - Indexes: world entries, character data, ref docs    │ │
│  └────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  HTTP API (axum)                                       │ │
│  │  - GET /health                                         │ │
│  │  - GET /conversations/:id                              │ │
│  │  - GET /config                                         │ │
│  │  - PUT /config                                         │ │
│  │  - POST /documents (upload reference docs)             │ │
│  │  - GET /documents                                      │ │
│  │  - DELETE /documents/:id                               │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

---

## Component Design

### 1. Rust Sidecar — Agent Loop (`src/agent/loop.rs`)

The core loop follows the Codex pattern (reference: `codex-rs/core/src/agent/`):

```rust
pub async fn run_turn(
    ctx: &mut AgentContext,
    user_message: String,
    tx: &WebSocketSender,
) -> Result<()> {
    ctx.conversation.push(Message::user(user_message));
    
    let mut iterations = 0;
    loop {
        if iterations >= ctx.config.max_iterations {
            tx.send(WsEvent::Error("Max iterations reached".into())).await?;
            break;
        }
        iterations += 1;

        // Build messages array
        let messages = ctx.build_messages();
        
        // Stream LLM response
        let response = ctx.llm_client
            .chat_completion_stream(&messages, &ctx.tools)
            .await?;

        // Process streamed response
        match response {
            LlmResponse::Text(content) => {
                // Stream tokens to frontend as they arrive
                tx.send(WsEvent::Token(content.clone())).await?;
                ctx.conversation.push(Message::assistant(content));
                break; // Done — final response
            }
            LlmResponse::ToolCall(tool_call) => {
                // Notify frontend
                tx.send(WsEvent::ToolStart(tool_call.name.clone())).await?;
                
                // Execute tool
                let result = ctx.tool_dispatcher.execute(&tool_call).await;
                
                // Notify frontend
                tx.send(WsEvent::ToolEnd(tool_call.name.clone(), result.success)).await?;
                
                // Append to conversation
                ctx.conversation.push(Message::tool_call(tool_call));
                ctx.conversation.push(Message::tool_result(result));
                
                // Continue loop — LLM will see the tool result
            }
        }
    }
    
    // Persist conversation
    ctx.db.save_conversation(&ctx.conversation).await?;
    Ok(())
}
```

### 2. Rust Sidecar — LLM Client (`src/llm/client.rs`)

Handles OpenAI-compatible chat completion with SSE streaming:

```rust
pub struct LlmClient {
    http: reqwest::Client,
    config: ModelProfile,
}

impl LlmClient {
    /// Stream a chat completion, yielding tokens or a tool call
    pub async fn chat_completion_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse> {
        let body = json!({
            "model": self.config.model,
            "messages": messages,
            "tools": tools,
            "stream": true,
            "temperature": self.config.temperature,
        });

        let response = self.http
            .post(&format!("{}/chat/completions", self.config.base_url))
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await?;

        // Parse SSE stream, accumulate tokens or detect tool_calls
        self.process_sse_stream(response).await
    }
}
```

### 3. Rust Sidecar — Tool Dispatcher (`src/tools/dispatcher.rs`)

Routes tool calls to implementations and validates arguments:

```rust
pub struct ToolDispatcher {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolDispatcher {
    pub async fn execute(&self, call: &ToolCall) -> ToolResult {
        let Some(tool) = self.tools.get(&call.name) else {
            return ToolResult::error(format!("Unknown tool: {}", call.name));
        };

        // Validate arguments against schema
        if let Err(e) = tool.validate_args(&call.arguments) {
            return ToolResult::error(format!("Invalid arguments: {}", e));
        }

        // Execute
        match tool.execute(call.arguments.clone()).await {
            Ok(data) => ToolResult::success(data),
            Err(e) => ToolResult::error(e.to_string()),
        }
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    fn validate_args(&self, args: &serde_json::Value) -> Result<()>;
    async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value>;
}
```

### 4. Rust Sidecar — Context Builder (`src/context/builder.rs`)

Assembles the chat-completion messages array:

```rust
pub struct ContextBuilder {
    system_prompt: String,      // ENI's personality card
    post_card_prompt: String,   // User-editable addition
    max_tokens: usize,
    tokenizer: Tokenizer,
}

impl ContextBuilder {
    pub fn build_messages(
        &self,
        conversation: &[Message],
        tools: &[ToolDefinition],
        relevant_chunks: &[DocumentChunk],
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // System message: ENI card + post-card + relevant context
        let mut system_content = self.system_prompt.clone();
        if !self.post_card_prompt.is_empty() {
            system_content.push_str("\n\n");
            system_content.push_str(&self.post_card_prompt);
        }
        if !relevant_chunks.is_empty() {
            system_content.push_str("\n\n## Reference Context\n");
            for chunk in relevant_chunks {
                system_content.push_str(&format!("\n[{}]: {}\n", chunk.source, chunk.content));
            }
        }
        messages.push(ChatMessage::system(system_content));

        // Conversation history (truncated to fit budget)
        let history = self.truncate_to_budget(conversation);
        for msg in history {
            messages.push(msg.to_chat_message());
        }

        messages
    }

    fn truncate_to_budget(&self, conversation: &[Message]) -> &[Message] {
        // Keep system prompt + last 4 messages minimum
        // Remove oldest messages until under token budget
        // ...
    }
}
```

### 5. Rust Sidecar — Version History (`src/versioning/mod.rs`)

Snapshots entities before modification:

```rust
pub struct VersionStore {
    db: SqlitePool,
}

impl VersionStore {
    /// Save a snapshot before a write operation
    pub async fn snapshot(&self, entity_type: &str, entity_id: &str, data: &Value) -> Result<()> {
        sqlx::query("INSERT INTO entity_versions (entity_type, entity_id, data, created_at) VALUES (?, ?, ?, ?)")
            .bind(entity_type)
            .bind(entity_id)
            .bind(serde_json::to_string(data)?)
            .bind(chrono::Utc::now())
            .execute(&self.db)
            .await?;

        // Prune old versions (keep last 20)
        self.prune(entity_type, entity_id, 20).await?;
        Ok(())
    }

    /// Revert to the most recent snapshot
    pub async fn undo(&self, entity_type: &str, entity_id: &str) -> Result<Value> {
        // Pop the most recent version and return it
    }
}
```

### 6. Rust Sidecar — Search Index (`src/search/mod.rs`)

BM25 full-text search with optional vector embeddings:

```rust
pub struct SearchIndex {
    tantivy_index: tantivy::Index,
    // Optional: vector store for semantic search
    embeddings: Option<FastEmbedModel>,
}

impl SearchIndex {
    /// Index a document (world entry, character data, reference doc chunk)
    pub fn index_document(&self, doc: &IndexableDocument) -> Result<()> { ... }

    /// Search by text query (BM25)
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> { ... }

    /// Semantic search (if embeddings available)
    pub fn semantic_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> { ... }
}
```

### 7. Server Plugin — Sidecar Lifecycle Manager (`index.js`)

The Node.js server plugin runs inside SillyTavern's process and manages the sidecar binary:

```javascript
const { spawn } = require('child_process');
const path = require('path');
const http = require('http');

const SIDECAR_PORT = 7842;
const HEALTH_POLL_INTERVAL = 500; // ms
const HEALTH_POLL_TIMEOUT = 10000; // ms
const SHUTDOWN_GRACE_PERIOD = 5000; // ms

let sidecarProcess = null;
let restartAttempts = 0;
const MAX_RESTART_ATTEMPTS = 1;

function getBinaryName() {
    const platform = process.platform; // 'darwin', 'linux', 'win32'
    const arch = process.arch; // 'arm64', 'x64'
    const ext = platform === 'win32' ? '.exe' : '';
    return `eni-sidecar-${platform}-${arch}${ext}`;
}

function spawnSidecar(configPath) {
    const binPath = path.join(__dirname, 'bin', getBinaryName());
    
    sidecarProcess = spawn(binPath, ['--port', SIDECAR_PORT, '--config', configPath], {
        stdio: ['ignore', 'pipe', 'pipe'],
    });

    sidecarProcess.stdout.on('data', (data) => {
        console.log(`[eni-sidecar] ${data.toString().trim()}`);
    });

    sidecarProcess.stderr.on('data', (data) => {
        console.error(`[eni-sidecar] ${data.toString().trim()}`);
    });

    sidecarProcess.on('exit', (code) => {
        console.log(`[eni-sidecar] Process exited with code ${code}`);
        if (code !== 0 && restartAttempts < MAX_RESTART_ATTEMPTS) {
            restartAttempts++;
            console.log(`[eni-sidecar] Attempting restart (${restartAttempts}/${MAX_RESTART_ATTEMPTS})...`);
            spawnSidecar(configPath);
        }
    });
}

async function waitForHealth() {
    const start = Date.now();
    while (Date.now() - start < HEALTH_POLL_TIMEOUT) {
        try {
            const res = await fetch(`http://127.0.0.1:${SIDECAR_PORT}/health`);
            if (res.ok) return true;
        } catch {}
        await new Promise(r => setTimeout(r, HEALTH_POLL_INTERVAL));
    }
    return false;
}

function shutdown() {
    if (!sidecarProcess) return;
    sidecarProcess.kill('SIGTERM');
    setTimeout(() => {
        if (sidecarProcess && !sidecarProcess.killed) {
            sidecarProcess.kill('SIGKILL');
        }
    }, SHUTDOWN_GRACE_PERIOD);
}

// Extension lifecycle hooks (called by ST)
module.exports = {
    init: async (configPath) => {
        spawnSidecar(configPath);
        const healthy = await waitForHealth();
        if (!healthy) throw new Error('Sidecar failed to start');
    },
    exit: () => shutdown(),
};
```

**Binary distribution structure:**
```
extension-root/
├── index.js              # Server plugin (above)
├── bin/
│   ├── eni-sidecar-darwin-arm64
│   ├── eni-sidecar-darwin-x64
│   ├── eni-sidecar-linux-x64
│   └── eni-sidecar-win32-x64.exe
├── dist/                 # Svelte frontend build output
│   ├── index.js
│   └── index.css
└── config.default.toml   # Default sidecar config template
```

### 8. Svelte Frontend — Component Structure

**Refer to `mockups/eni-ui-mockup.html` for the canonical visual reference when implementing these components.**

```
src/
├── lib/
│   ├── stores/
│   │   ├── connection.ts      # WebSocket connection state
│   │   ├── conversation.ts    # Messages, streaming state
│   │   ├── ui.ts              # Panel mode, active tab, preview state
│   │   └── config.ts          # Model profiles, settings
│   ├── ws/
│   │   └── client.ts          # WebSocket client (connect, send, receive)
│   └── utils/
│       ├── markdown.ts        # Markdown rendering
│       └── theme.ts           # CSS variable injection from ST
├── components/
│   ├── PanelShell.svelte      # Root: resize handle, header, layout
│   ├── ChatPane.svelte        # Message list, input, status bar
│   ├── MessageBubble.svelte   # Plain bubble (no avatar, user/assistant styling)
│   ├── ToolCallCard.svelte    # Compact inline tool execution card
│   ├── ThinkingBlock.svelte   # Collapsible thinking/reasoning block
│   ├── RightPane.svelte       # Tabbed container (Character/World/Post-History/Persona/Settings)
│   ├── tabs/
│   │   ├── CharacterTab.svelte    # Character card preview
│   │   ├── WorldTab.svelte        # World entries list with keyword tags
│   │   ├── PostHistoryTab.svelte  # Post-history rules in monospace
│   │   ├── PersonaTab.svelte      # User persona preview
│   │   └── SettingsTab.svelte     # Model config, post-card prompt, docs, connection
│   ├── ModelSelector.svelte   # Dropdown for model switching (in status bar)
│   └── UndoToast.svelte       # Undo indicator after writes
├── App.svelte                 # Entry point
└── main.ts                    # ST extension bootstrap
```

### 9. WebSocket Protocol

Messages between frontend and sidecar:

**Frontend → Sidecar:**
```typescript
type ClientMessage =
  | { type: "user_message"; content: string }
  | { type: "cancel" }
  | { type: "switch_model"; profile: string }
  | { type: "new_conversation" }
  | { type: "undo"; entity_type: string; entity_id: string }
  | { type: "update_config"; key: string; value: any }
```

**Sidecar → Frontend:**
```typescript
type ServerMessage =
  | { type: "token"; content: string }                          // Streamed text token
  | { type: "thinking"; content: string }                       // Thinking/reasoning content (for collapsible block)
  | { type: "message_complete"; id: string }                    // Final message done
  | { type: "tool_start"; name: string; description: string }   // Tool execution beginning
  | { type: "tool_end"; name: string; success: boolean }        // Tool execution complete
  | { type: "preview"; tab: "character"|"world"|"posthistory"|"persona"; data: any }  // Update a preview tab
  | { type: "error"; message: string }
  | { type: "status"; state: "idle" | "thinking" | "tool_executing" }
  | { type: "undo_available"; entity_type: string; entity_id: string; summary: string }
  | { type: "system_message"; content: string }
  | { type: "config_updated"; key: string }
```

### 10. SQLite Schema

```sql
-- Conversations
CREATE TABLE conversations (
    id TEXT PRIMARY KEY,
    title TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    archived INTEGER DEFAULT 0
);

CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT REFERENCES conversations(id),
    role TEXT NOT NULL,  -- 'user', 'assistant', 'tool_call', 'tool_result', 'system'
    content TEXT NOT NULL,
    metadata TEXT,  -- JSON: tool call details, token counts, etc.
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Projects & Tasks
CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    metadata TEXT,  -- JSON: genre, setting, tone
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id),
    title TEXT NOT NULL,
    status TEXT DEFAULT 'planned',  -- planned, in_progress, complete
    description TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- World Entries
CREATE TABLE world_entries (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id),
    label TEXT NOT NULL,
    content TEXT NOT NULL,
    keywords TEXT,  -- comma-separated for lorebook matching
    metadata TEXT,  -- JSON
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Reference Documents
CREATE TABLE reference_documents (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id),
    filename TEXT NOT NULL,
    content TEXT NOT NULL,
    size_bytes INTEGER,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE document_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT REFERENCES reference_documents(id),
    content TEXT NOT NULL,
    chunk_index INTEGER,
    embedding BLOB  -- optional: vector embedding
);

-- Version History
CREATE TABLE entity_versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type TEXT NOT NULL,  -- 'character', 'world_entry'
    entity_id TEXT NOT NULL,
    data TEXT NOT NULL,  -- JSON snapshot
    summary TEXT,  -- human-readable change description
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Configuration
CREATE TABLE config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE model_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    api_key TEXT NOT NULL,
    model TEXT NOT NULL,
    temperature REAL DEFAULT 0.7,
    max_tokens INTEGER DEFAULT 4096,
    is_default INTEGER DEFAULT 0
);
```

### 11. Rust Crate Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
axum = "0.8"
tokio-tungstenite = "0.24"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.32", features = ["bundled"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
tantivy = "0.22"
tiktoken-rs = "0.6"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
toml = "0.8"
tracing = "0.1"
tracing-subscriber = "0.3"
anyhow = "1"
async-trait = "0.1"
futures = "0.3"
eventsource-stream = "0.2"  # SSE parsing
```

### 12. Svelte Build & Bundling

The Svelte frontend is built with Vite and outputs a single JS + CSS bundle that ST loads as an extension:

```
vite.config.ts:
- Input: src/main.ts
- Output: dist/index.js + dist/index.css
- Format: IIFE (immediately invoked, no module system — ST loads via script tag)
- Svelte plugin for .svelte compilation
- CSS scoped to .wb-root container (same isolation strategy as before)
```

---

## Data Flow Diagrams

### User sends a message (happy path)

```
User types "Build me a character named Kael"
    │
    ▼
Svelte: sends { type: "user_message", content: "..." } via WebSocket
    │
    ▼
Sidecar: receives message, appends to conversation
    │
    ▼
Sidecar: Context Builder assembles messages array
    │  (system prompt + post-card + history + tool defs)
    ▼
Sidecar: LLM Client calls POST /chat/completions (stream: true)
    │
    ▼
LLM returns tool_call: write_character({ name: "Kael", ... })
    │
    ▼
Sidecar: sends { type: "tool_start", name: "write_character" } to frontend
    │
    ▼
Sidecar: VersionStore.snapshot() — saves previous state
    │
    ▼
Sidecar: Tool executes — writes to ST via REST API
    │
    ▼
Sidecar: sends { type: "tool_end", name: "write_character", success: true }
Sidecar: sends { type: "undo_available", entity_type: "character", ... }
    │
    ▼
Sidecar: appends tool_call + tool_result to conversation
    │
    ▼
Sidecar: calls LLM again with updated context
    │
    ▼
LLM returns tool_call: show_preview({ content_type: "character_card", ... })
    │
    ▼
Sidecar: sends { type: "preview", content_type: "character_card", data: {...} }
    │
    ▼
Svelte: opens Preview Pane, renders character card
    │
    ▼
Sidecar: calls LLM again
    │
    ▼
LLM returns text: "Done! I've created Kael. Here's what I set up..."
    │
    ▼
Sidecar: streams tokens → { type: "token", content: "Done" }, { type: "token", content: "!" }, ...
    │
    ▼
Svelte: renders tokens incrementally in chat
    │
    ▼
Sidecar: sends { type: "message_complete", id: "..." }
Sidecar: persists conversation to SQLite
```

---

## Key Design Decisions

1. **Rust sidecar over browser-side agent loop** — Enables true SSE streaming, non-blocking tool execution, fast token counting, and local persistence without browser storage limits.

2. **Direct LLM API calls over ST's generateRaw** — ST's generateRaw doesn't stream tokens. By calling the API directly, we get real-time token streaming and full control over the request format.

3. **Svelte over React** — Compiles to vanilla JS with no runtime overhead. Critical for a sidebar panel that needs to feel instant, especially during streaming where DOM updates happen on every token.

4. **BM25 (tantivy) over pure vector search** — BM25 is fast, requires no embedding model, and works well for keyword-heavy creative writing content. Vector search is optional for users who want semantic similarity.

5. **SQLite over IndexedDB** — Server-side persistence is more reliable, supports complex queries, and doesn't have browser storage limits. Also enables the sidecar to maintain state independently of the browser.

6. **WebSocket over HTTP polling** — Bidirectional real-time communication is essential for streaming tokens and tool status updates without latency.

7. **OpenAI function-calling format** — Using the standard `tools` array in chat completions means we get native tool call parsing from the API (no regex needed) and compatibility with any OpenAI-compatible provider.

8. **Version snapshots over event sourcing** — Simple JSON snapshots per entity are easier to implement and reason about than a full event log. 20 versions per entity is sufficient for creative undo workflows.

9. **Server plugin auto-spawn over manual sidecar management** — The ST extension's Node.js server plugin spawns the Rust sidecar as a child process automatically. This eliminates user friction (no separate terminal, no remembering to start it) while keeping the architecture cleanly separated. The trade-off is shipping ~10-20MB of platform-specific binaries per target, but it makes the extension feel like a single integrated product.

10. **GitHub Releases binary distribution over in-repo binaries** — Pre-compiled sidecar binaries are built in CI and attached to GitHub Releases rather than committed to the repo. The server plugin downloads the correct binary on first run if it's missing. This keeps the extension repo lightweight (~KB instead of ~60MB of binaries) and ensures users always get the correct platform binary without manual compilation.

---

## CI Pipeline — Cross-Platform Binary Builds

The Rust sidecar must be compiled for each target platform since SillyTavern has no mechanism to compile native code during extension installation. A GitHub Actions CI pipeline handles this automatically.

### Trigger

The pipeline triggers on version tags (`v*`). When a developer pushes a tag like `v0.1.0`, the workflow builds binaries for all supported platforms and attaches them to a GitHub Release.

### Target Platforms

| Target Triple | Runner | Output Binary |
|---|---|---|
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | `eni-sidecar-linux-x64` |
| `x86_64-apple-darwin` | `macos-latest` | `eni-sidecar-darwin-x64` |
| `aarch64-apple-darwin` | `macos-latest` | `eni-sidecar-darwin-arm64` |
| `x86_64-pc-windows-msvc` | `windows-latest` | `eni-sidecar-win32-x64.exe` |

### Workflow File (`.github/workflows/build-sidecar.yml`)

```yaml
name: Build Sidecar Binaries

on:
  push:
    tags:
      - 'v*'

jobs:
  build:
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
            binary_name: eni-sidecar-linux-x64
          - target: x86_64-apple-darwin
            os: macos-latest
            binary_name: eni-sidecar-darwin-x64
          - target: aarch64-apple-darwin
            os: macos-latest
            binary_name: eni-sidecar-darwin-arm64
          - target: x86_64-pc-windows-msvc
            os: windows-latest
            binary_name: eni-sidecar-win32-x64.exe

    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Build release binary
        run: cargo build --release --target ${{ matrix.target }}
        working-directory: eni-sidecar

      - name: Rename binary
        shell: bash
        run: |
          src="eni-sidecar/target/${{ matrix.target }}/release/eni-sidecar"
          if [ "${{ matrix.os }}" = "windows-latest" ]; then
            src="${src}.exe"
          fi
          cp "$src" "${{ matrix.binary_name }}"

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.binary_name }}
          path: ${{ matrix.binary_name }}

  release:
    needs: build
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            eni-sidecar-linux-x64/eni-sidecar-linux-x64
            eni-sidecar-darwin-x64/eni-sidecar-darwin-x64
            eni-sidecar-darwin-arm64/eni-sidecar-darwin-arm64
            eni-sidecar-win32-x64.exe/eni-sidecar-win32-x64.exe
```

### Server Plugin — Binary Auto-Download

The server plugin (`index.js`) checks for the sidecar binary on startup. If missing, it downloads the correct one from the latest GitHub Release:

```javascript
const fs = require('fs');
const path = require('path');
const https = require('https');

const GITHUB_REPO = 'your-username/eni-world-builder'; // configure this
const BIN_DIR = path.join(__dirname, 'bin');

async function ensureBinary() {
    const binaryName = getBinaryName();
    const binaryPath = path.join(BIN_DIR, binaryName);

    if (fs.existsSync(binaryPath)) return binaryPath;

    console.log(`[eni-sidecar] Binary not found. Downloading ${binaryName}...`);
    fs.mkdirSync(BIN_DIR, { recursive: true });

    // Fetch latest release from GitHub API
    const release = await fetchJson(
        `https://api.github.com/repos/${GITHUB_REPO}/releases/latest`
    );
    const asset = release.assets.find(a => a.name === binaryName);
    if (!asset) {
        throw new Error(`No binary available for this platform: ${binaryName}`);
    }

    // Download the binary
    await downloadFile(asset.browser_download_url, binaryPath);

    // Make executable (unix)
    if (process.platform !== 'win32') {
        fs.chmodSync(binaryPath, 0o755);
    }

    console.log(`[eni-sidecar] Downloaded ${binaryName} successfully.`);
    return binaryPath;
}
```

### Release Workflow (Developer)

1. Develop and test locally
2. Tag a release: `git tag v0.1.0 && git push --tags`
3. GitHub Actions builds all 4 platform binaries (~3-5 minutes)
4. Binaries appear on the GitHub Releases page
5. Users install/update the extension → server plugin auto-downloads the correct binary on first run
