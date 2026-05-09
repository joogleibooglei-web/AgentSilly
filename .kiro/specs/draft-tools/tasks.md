# Implementation Plan: Draft-Based World Info and Post-History Tools

## Overview

Replace the existing SQLite-backed `world_entries` and `post_history` tools with an ephemeral draft-file workflow. The agent creates, edits, and finalizes plain-text drafts stored at `/tmp/eni-sidecar/`. Real-time preview is pushed via WebSocket events, and finalization persists content to SillyTavern's character card fields. Frontend tabs are updated to render monolithic text blocks.

## Tasks

- [x] 1. Add SessionContext and draft file utilities
  - [x] 1.1 Create `eni-sidecar/src/agent/session.rs` with `SessionContext` struct
    - Define `SessionContext` with `last_avatar_url: Option<String>`
    - Define `SharedSessionContext` as `Arc<Mutex<SessionContext>>`
    - Add `pub mod session;` and re-export `SharedSessionContext` from `eni-sidecar/src/agent/mod.rs`
    - _Requirements: 3.2, 3.6, 6.2, 6.6_

  - [x] 1.2 Create `eni-sidecar/src/tools/draft_file.rs` with draft I/O utilities
    - Implement constants `DRAFT_DIR`, `WORLD_DRAFT_PATH`, `POST_HISTORY_DRAFT_PATH`
    - Implement `ensure_draft_dir()`, `read_draft()`, `write_draft()`, `delete_draft()`, `str_replace_first()`
    - Add `pub mod draft_file;` to `eni-sidecar/src/tools/mod.rs`
    - _Requirements: 1.1, 1.4, 2.1, 4.1, 4.4, 5.1_

  - [ ]* 1.3 Write property test for `str_replace_first`
    - **Property 3: Edit replaces first occurrence only**
    - **Validates: Requirements 2.1, 5.1**

- [x] 2. Implement world draft tools
  - [x] 2.1 Implement `CreateWorldDraftTool` in `eni-sidecar/src/tools/drafts.rs`
    - Implement `Tool` trait with `name()`, `description()`, `parameters_schema()`, `validate_args()`, `execute()`
    - Accept `event_tx: mpsc::Sender<WsEvent>` for sending Preview events
    - Parameters schema: `{ content: string }` (required)
    - On execute: call `write_draft`, send Preview event with `tab: "world"`, return success JSON with optional warning if overwritten
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 10.1, 10.2, 10.3_

  - [x] 2.2 Implement `EditWorldDraftTool` in `eni-sidecar/src/tools/drafts.rs`
    - Implement `Tool` trait
    - Parameters schema: `{ old_text: string, new_text: string }` (both required)
    - On execute: read draft (error if missing), call `str_replace_first` (error if not found), write back, send Preview event
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 10.1, 10.2, 10.3_

  - [x] 2.3 Implement `FinalizeWorldInfoTool` in `eni-sidecar/src/tools/drafts.rs`
    - Implement `Tool` trait
    - Accept `st_client`, `session_ctx`, `event_tx`
    - Parameters schema: `{}` (no parameters)
    - On execute: read draft (error if missing), read session context avatar (error if missing), fetch character, prepend draft to description with `\n\n` separator, write via StClient, delete draft, send Preview with null data
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 10.1, 10.2, 10.3_

  - [ ]* 2.4 Write property tests for world draft tools
    - **Property 1: Draft creation round-trip**
    - **Property 2: Draft overwrite replaces content and warns**
    - **Validates: Requirements 1.1, 1.2, 1.3, 1.5, 4.1, 4.2, 4.3, 4.5**

  - [ ]* 2.5 Write property test for world info finalization
    - **Property 5: World info finalization prepends draft to description**
    - **Validates: Requirements 3.3**

- [x] 3. Implement post-history draft tools
  - [x] 3.1 Implement `CreatePostHistoryDraftTool` in `eni-sidecar/src/tools/drafts.rs`
    - Implement `Tool` trait
    - Mirrors `CreateWorldDraftTool` but uses `POST_HISTORY_DRAFT_PATH` and `tab: "posthistory"`
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5, 10.1, 10.2, 10.3_

  - [x] 3.2 Implement `EditPostHistoryDraftTool` in `eni-sidecar/src/tools/drafts.rs`
    - Implement `Tool` trait
    - Mirrors `EditWorldDraftTool` but uses `POST_HISTORY_DRAFT_PATH` and `tab: "posthistory"`
    - _Requirements: 5.1, 5.2, 5.3, 5.4, 10.1, 10.2, 10.3_

  - [x] 3.3 Implement `FinalizePostHistoryTool` in `eni-sidecar/src/tools/drafts.rs`
    - Implement `Tool` trait
    - Accept `st_client`, `session_ctx`, `event_tx`
    - On execute: read draft (error if missing), read session context avatar (error if missing), write draft content to `post_history_instructions` field (full replacement), delete draft, send Preview with null data
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 6.7, 10.1, 10.2, 10.3_

  - [ ]* 3.4 Write property test for post-history finalization
    - **Property 6: Post-history finalization replaces field entirely**
    - **Validates: Requirements 6.3**

- [x] 4. Checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [x] 5. Integrate session context and wire tools into dispatcher
  - [x] 5.1 Modify `eni-sidecar/src/tools/read_character.rs` to accept and update `SharedSessionContext`
    - Add `session_ctx: SharedSessionContext` field to `ReadCharacterTool`
    - After successful character read, set `session_ctx.last_avatar_url = character.avatar`
    - Update constructor to accept `SharedSessionContext`
    - _Requirements: 3.2, 6.2_

  - [x] 5.2 Modify `eni-sidecar/src/tools/write_character.rs` to accept and update `SharedSessionContext`
    - Add `session_ctx: SharedSessionContext` field to `WriteCharacterTool`
    - After successful character write, set `session_ctx.last_avatar_url = resolved_avatar_url`
    - Update constructor to accept `SharedSessionContext`
    - _Requirements: 3.2, 6.2_

  - [x] 5.3 Remove old tools and register new tools in `eni-sidecar/src/ws/server.rs`
    - Remove `ReadPostHistoryTool`, `WritePostHistoryTool`, `ReadWorldEntriesTool`, `WriteWorldEntryTool` registrations
    - Create `SharedSessionContext` per-connection
    - Pass `SharedSessionContext` to `ReadCharacterTool`, `WriteCharacterTool`, and all finalize tools
    - Register all six new draft tools: `CreateWorldDraftTool`, `EditWorldDraftTool`, `FinalizeWorldInfoTool`, `CreatePostHistoryDraftTool`, `EditPostHistoryDraftTool`, `FinalizePostHistoryTool`
    - Update imports to remove old tools and add new ones
    - _Requirements: 7.4, 7.5_

  - [x] 5.4 Remove old tool source files and update `eni-sidecar/src/tools/mod.rs`
    - Delete `eni-sidecar/src/tools/world_entries.rs`
    - Delete `eni-sidecar/src/tools/post_history.rs`
    - Remove `pub mod world_entries;` and `pub mod post_history;` from mod.rs
    - Remove old re-exports (`ReadWorldEntriesTool`, `WriteWorldEntryTool`, `ReadPostHistoryTool`, `WritePostHistoryTool`)
    - Add `pub mod drafts;` and re-export the six new tool structs
    - _Requirements: 7.1, 7.2, 7.3_

  - [ ]* 5.5 Write property test for tool schema validity and argument validation
    - **Property 7: Tool schema validity and argument validation**
    - **Validates: Requirements 10.2, 10.3**

- [x] 6. Checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [x] 7. Update frontend tabs
  - [x] 7.1 Update `frontend/src/components/tabs/WorldTab.svelte`
    - Remove array-of-cards rendering logic
    - Subscribe to `ui.previewData.world` and render as a single `<pre>` text block when string data is present
    - Show empty-state placeholder when data is null
    - _Requirements: 8.1, 8.2, 8.3_

  - [x] 7.2 Update `frontend/src/components/tabs/PostHistoryTab.svelte`
    - Remove structured fields rendering (narration_style, formatting_rules, tone_keywords)
    - Subscribe to `ui.previewData.posthistory` and render as a single `<pre>` text block when string data is present
    - Show empty-state placeholder when data is null
    - _Requirements: 9.1, 9.2, 9.3_

- [x] 8. Final checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties from the design document
- The project uses Rust (backend) and Svelte/TypeScript (frontend)
- All six draft tools live in a single `drafts.rs` file since they share dependencies and patterns
- The `draft_file.rs` module is a pure utility with no tool trait implementations

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2"] },
    { "id": 1, "tasks": ["1.3", "2.1", "3.1"] },
    { "id": 2, "tasks": ["2.2", "2.3", "3.2", "3.3"] },
    { "id": 3, "tasks": ["2.4", "2.5", "3.4", "5.1", "5.2"] },
    { "id": 4, "tasks": ["5.3", "5.4"] },
    { "id": 5, "tasks": ["5.5", "7.1", "7.2"] }
  ]
}
```
