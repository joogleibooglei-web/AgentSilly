# Design Document: Draft-Based World Info and Post-History Tools

## Architecture Overview

This feature replaces the existing SQLite-backed world entry and post-history tools with an ephemeral draft-file workflow. The agent creates, edits, and finalizes plain-text drafts stored at fixed paths under `/tmp/eni-sidecar/`. Real-time preview is pushed to the frontend via the existing `WsEvent::Preview` WebSocket event. Finalization persists content to SillyTavern's character card fields through `StClient::edit_character`.

### High-Level Flow

```
Agent ──create_*_draft──► /tmp/eni-sidecar/*.txt ──Preview event──► Frontend Tab
Agent ──edit_*_draft────► /tmp/eni-sidecar/*.txt ──Preview event──► Frontend Tab
Agent ──finalize_*──────► StClient::edit_character ──delete file──► (cleanup)
```

### Key Design Decisions

- **Ephemeral storage**: Draft files live at `/tmp/eni-sidecar/` and do not survive process restarts. No database involvement.
- **Monolithic text**: Both world info and post-history are single text blobs, not arrays or structured objects.
- **str_replace editing**: The `edit_*_draft` tools use `old_text → new_text` first-occurrence replacement, matching the pattern used by code editors.
- **Session context for finalization target**: A new `SessionContext` struct tracks the last character `avatar_url` the agent interacted with (via `read_character` or `write_character`). Finalization uses this to determine which character to write to.
- **Old tools removed entirely**: `world_entries.rs`, `post_history.rs`, and their dispatcher registrations are deleted.

---

## Components

### 1. `SessionContext` (New)

A shared, per-connection struct that tracks session-level state needed by draft tools.

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

/// Tracks session-level state across tool invocations within a single connection.
#[derive(Debug, Clone, Default)]
pub struct SessionContext {
    /// The avatar_url of the last character the agent read or wrote.
    /// Used by finalize tools to determine the target character.
    pub last_avatar_url: Option<String>,
}

pub type SharedSessionContext = Arc<Mutex<SessionContext>>;
```

**Location**: `eni-sidecar/src/agent/session.rs` (new file, re-exported from `agent/mod.rs`)

**Integration**: Created per-connection in `ws/server.rs::handle_connection`, passed to `ReadCharacterTool`, `WriteCharacterTool`, and all finalize tools. The read/write character tools update `last_avatar_url` after successful operations.

---

### 2. Draft File Module (New)

A small utility module encapsulating draft file I/O operations.

```rust
// eni-sidecar/src/tools/draft_file.rs

use std::path::{Path, PathBuf};
use anyhow::Result;
use tokio::fs;

/// Base directory for all draft files.
pub const DRAFT_DIR: &str = "/tmp/eni-sidecar";

/// Fixed path for the world info draft.
pub const WORLD_DRAFT_PATH: &str = "/tmp/eni-sidecar/world_draft.txt";

/// Fixed path for the post-history draft.
pub const POST_HISTORY_DRAFT_PATH: &str = "/tmp/eni-sidecar/post_history_draft.txt";

/// Ensure the draft directory exists, creating it if necessary.
pub async fn ensure_draft_dir() -> Result<()> {
    fs::create_dir_all(DRAFT_DIR).await?;
    Ok(())
}

/// Read a draft file. Returns None if the file does not exist.
pub async fn read_draft(path: &str) -> Result<Option<String>> {
    match fs::read_to_string(path).await {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Write content to a draft file. Creates the directory if needed.
/// Returns true if a previous draft existed (was overwritten).
pub async fn write_draft(path: &str, content: &str) -> Result<bool> {
    ensure_draft_dir().await?;
    let existed = Path::new(path).exists();
    fs::write(path, content).await?;
    Ok(existed)
}

/// Delete a draft file. No-op if the file doesn't exist.
pub async fn delete_draft(path: &str) -> Result<()> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Replace the first occurrence of `old_text` in `content` with `new_text`.
/// Returns the modified string, or None if `old_text` was not found.
pub fn str_replace_first(content: &str, old_text: &str, new_text: &str) -> Option<String> {
    if let Some(pos) = content.find(old_text) {
        let mut result = String::with_capacity(content.len() - old_text.len() + new_text.len());
        result.push_str(&content[..pos]);
        result.push_str(new_text);
        result.push_str(&content[pos + old_text.len()..]);
        Some(result)
    } else {
        None
    }
}
```

---

### 3. Tool Implementations (New)

All six tools are in a single file `eni-sidecar/src/tools/drafts.rs` since they share the same dependencies and patterns.

#### 3.1 `CreateWorldDraftTool`

```rust
pub struct CreateWorldDraftTool {
    event_tx: mpsc::Sender<WsEvent>,
}
```

**Parameters**: `{ "content": string }` (required)

**Behavior**:
1. Call `write_draft(WORLD_DRAFT_PATH, content)` — returns whether file existed
2. Send `Preview { tab: "world", data: Value::String(content) }`
3. Return `{ success: true, path: WORLD_DRAFT_PATH, warning?: "Previous draft was replaced" }`

#### 3.2 `EditWorldDraftTool`

```rust
pub struct EditWorldDraftTool {
    event_tx: mpsc::Sender<WsEvent>,
}
```

**Parameters**: `{ "old_text": string, "new_text": string }` (both required)

**Behavior**:
1. Read draft via `read_draft(WORLD_DRAFT_PATH)` — error if None
2. Call `str_replace_first(content, old_text, new_text)` — error if None (not found)
3. Write result back to file
4. Send `Preview { tab: "world", data: Value::String(new_content) }`
5. Return `{ success: true, path: WORLD_DRAFT_PATH }`

#### 3.3 `FinalizeWorldInfoTool`

```rust
pub struct FinalizeWorldInfoTool {
    st_client: Arc<Mutex<StClient>>,
    session_ctx: SharedSessionContext,
    event_tx: mpsc::Sender<WsEvent>,
}
```

**Parameters**: `{}` (no parameters)

**Behavior**:
1. Read draft via `read_draft(WORLD_DRAFT_PATH)` — error if None
2. Read `session_ctx.last_avatar_url` — error if None
3. Fetch current character via `st_client.get_character(avatar_url)`
4. Compute merged description: `format!("{}\n\n{}", draft_content, character.description)`
5. Call `st_client.edit_character(avatar_url, { "description": merged })`
6. Delete draft file
7. Send `Preview { tab: "world", data: Value::Null }`
8. Return `{ success: true, character: avatar_url, message: "World info finalized" }`

#### 3.4 `CreatePostHistoryDraftTool`

Mirrors `CreateWorldDraftTool` but uses `POST_HISTORY_DRAFT_PATH` and `tab: "posthistory"`.

#### 3.5 `EditPostHistoryDraftTool`

Mirrors `EditWorldDraftTool` but uses `POST_HISTORY_DRAFT_PATH` and `tab: "posthistory"`.

#### 3.6 `FinalizePostHistoryTool`

```rust
pub struct FinalizePostHistoryTool {
    st_client: Arc<Mutex<StClient>>,
    session_ctx: SharedSessionContext,
    event_tx: mpsc::Sender<WsEvent>,
}
```

**Parameters**: `{}` (no parameters)

**Behavior**:
1. Read draft via `read_draft(POST_HISTORY_DRAFT_PATH)` — error if None
2. Read `session_ctx.last_avatar_url` — error if None
3. Call `st_client.edit_character(avatar_url, { "post_history_instructions": draft_content })` — full replacement
4. Delete draft file
5. Send `Preview { tab: "posthistory", data: Value::Null }`
6. Return `{ success: true, character: avatar_url, message: "Post-history finalized" }`

---

### 4. Frontend Changes

#### WorldTab.svelte

Replace the array-of-cards rendering with a simple monolithic text display:

```svelte
<script lang="ts">
  import { onDestroy } from 'svelte';
  import { ui } from '../../lib/stores/ui';

  let content: string | null = $state(null);

  const unsubUi = ui.subscribe(($ui) => {
    const raw = $ui.previewData.world;
    content = typeof raw === 'string' ? raw : null;
  });

  onDestroy(unsubUi);
</script>

{#if content}
  <div class="world-tab">
    <pre class="draft-content">{content}</pre>
  </div>
{:else}
  <div class="tab-placeholder">
    <span class="placeholder-icon">🌍</span>
    <span class="placeholder-text">World Info</span>
    <span class="placeholder-hint">Ask ENI to draft world info to see it here.</span>
  </div>
{/if}
```

#### PostHistoryTab.svelte

Replace the structured fields rendering with the same monolithic text pattern:

```svelte
<script lang="ts">
  import { onDestroy } from 'svelte';
  import { ui } from '../../lib/stores/ui';

  let content: string | null = $state(null);

  const unsubUi = ui.subscribe(($ui) => {
    const raw = $ui.previewData.posthistory;
    content = typeof raw === 'string' ? raw : null;
  });

  onDestroy(unsubUi);
</script>

{#if content}
  <div class="posthistory-tab">
    <pre class="draft-content">{content}</pre>
  </div>
{:else}
  <div class="tab-placeholder">
    <span class="placeholder-icon">📝</span>
    <span class="placeholder-text">Post-History</span>
    <span class="placeholder-hint">Ask ENI to draft post-history instructions to see them here.</span>
  </div>
{/if}
```

---

## Interfaces

### Tool Parameter Schemas

| Tool | Parameters | Required |
|------|-----------|----------|
| `create_world_draft` | `{ content: string }` | `content` |
| `edit_world_draft` | `{ old_text: string, new_text: string }` | `old_text`, `new_text` |
| `finalize_world_info` | `{}` | (none) |
| `create_post_history_draft` | `{ content: string }` | `content` |
| `edit_post_history_draft` | `{ old_text: string, new_text: string }` | `old_text`, `new_text` |
| `finalize_post_history` | `{}` | (none) |

### WebSocket Events Emitted

| Event | Tab | Data | When |
|-------|-----|------|------|
| `Preview` | `"world"` | `String(content)` | After create/edit world draft |
| `Preview` | `"world"` | `Null` | After finalize world info |
| `Preview` | `"posthistory"` | `String(content)` | After create/edit post-history draft |
| `Preview` | `"posthistory"` | `Null` | After finalize post-history |

### Session Context Updates

| Tool | Action |
|------|--------|
| `read_character` | Sets `session_ctx.last_avatar_url = character.avatar` |
| `write_character` | Sets `session_ctx.last_avatar_url = resolved_avatar_url` |
| `finalize_world_info` | Reads `session_ctx.last_avatar_url` |
| `finalize_post_history` | Reads `session_ctx.last_avatar_url` |

---

## Data Model

### File System Layout

```
/tmp/eni-sidecar/
├── world_draft.txt          # Monolithic world info text (ephemeral)
└── post_history_draft.txt   # Monolithic post-history text (ephemeral)
```

- Files are plain UTF-8 text with no structure.
- Files are created on first `create_*_draft` call and deleted on successful `finalize_*` call.
- Files do not survive sidecar restarts (they're in `/tmp`).

### Session Context (In-Memory)

```rust
pub struct SessionContext {
    pub last_avatar_url: Option<String>,
}
```

- Lives for the duration of a WebSocket connection.
- Reset when the connection drops and a new one is established.
- Shared via `Arc<Mutex<SessionContext>>` among all tools in a connection.

---

## Error Handling

| Scenario | Tool | Error Message |
|----------|------|---------------|
| No draft file exists | `edit_*_draft`, `finalize_*` | "No draft exists at {path}. Use create_*_draft first." |
| `old_text` not found in draft | `edit_*_draft` | "Text not found in draft. The old_text does not match any content in the current draft." |
| No character in session | `finalize_*` | "No target character in session. Use read_character first to select a character." |
| File I/O failure | Any draft tool | "Failed to {read/write} draft file: {io_error}" |
| StClient failure | `finalize_*` | Propagates StClient error (connection, HTTP status, etc.) |

All errors are returned as `Err(anyhow::Error)` from `execute()`, which the dispatcher wraps in `ToolResult::error(message)`.

---

## Module Structure Changes

### Files to Add

- `eni-sidecar/src/agent/session.rs` — `SessionContext` struct
- `eni-sidecar/src/tools/draft_file.rs` — Draft file I/O utilities
- `eni-sidecar/src/tools/drafts.rs` — All six draft tool implementations

### Files to Remove

- `eni-sidecar/src/tools/world_entries.rs`
- `eni-sidecar/src/tools/post_history.rs`

### Files to Modify

- `eni-sidecar/src/agent/mod.rs` — Add `pub mod session;` and re-export
- `eni-sidecar/src/tools/mod.rs` — Remove old modules, add new modules, update re-exports
- `eni-sidecar/src/tools/read_character.rs` — Accept `SharedSessionContext`, update `last_avatar_url` on success
- `eni-sidecar/src/tools/write_character.rs` — Accept `SharedSessionContext`, update `last_avatar_url` on success
- `eni-sidecar/src/ws/server.rs` — Create `SessionContext` per-connection, pass to tools, remove old tool registrations, add new tool registrations
- `frontend/src/components/tabs/WorldTab.svelte` — Replace array rendering with monolithic text
- `frontend/src/components/tabs/PostHistoryTab.svelte` — Replace structured rendering with monolithic text

---

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system — essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

### Property 1: Draft creation round-trip

*For any* valid UTF-8 string `content`, invoking `create_world_draft` (or `create_post_history_draft`) with that content SHALL result in the corresponding draft file containing exactly `content`, the emitted Preview event's `data` field equaling `content`, and the tool response containing `success: true`.

**Validates: Requirements 1.1, 1.2, 1.5, 4.1, 4.2, 4.5**

### Property 2: Draft overwrite replaces content and warns

*For any* two valid UTF-8 strings `first` and `second`, if `create_world_draft` is called with `first` and then called again with `second`, the draft file SHALL contain exactly `second`, and the second response SHALL include a warning indicating the previous draft was replaced.

**Validates: Requirements 1.3, 4.3**

### Property 3: Edit replaces first occurrence only

*For any* draft content containing at least one occurrence of substring `old_text`, and any replacement string `new_text`, invoking `edit_world_draft` (or `edit_post_history_draft`) SHALL produce content where the first occurrence of `old_text` is replaced by `new_text` and all subsequent occurrences of `old_text` remain unchanged.

**Validates: Requirements 2.1, 2.2, 5.1, 5.2**

### Property 4: Edit with non-existent text returns error

*For any* draft content and any string `old_text` that is not a substring of the draft content, invoking `edit_world_draft` (or `edit_post_history_draft`) SHALL return an error and leave the draft file unchanged.

**Validates: Requirements 2.4, 5.4**

### Property 5: World info finalization prepends draft to description

*For any* non-empty draft content string `D` and any existing character description string `E`, the finalized description SHALL equal `D + "\n\n" + E`.

**Validates: Requirements 3.3**

### Property 6: Post-history finalization replaces field entirely

*For any* non-empty draft content string `D`, after finalization the character's `post_history_instructions` field SHALL equal exactly `D`, regardless of the previous field value.

**Validates: Requirements 6.3**

### Property 7: Tool schema validity and argument validation

*For any* of the six draft tools, `parameters_schema()` SHALL return a valid JSON Schema object with `type: "object"`, and *for any* arguments object missing a required field defined in that schema, `validate_args()` SHALL return an error.

**Validates: Requirements 10.2, 10.3**
