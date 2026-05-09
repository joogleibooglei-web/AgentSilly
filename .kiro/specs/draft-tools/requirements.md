# Requirements Document

## Introduction

This feature replaces the existing `read_world_entries` / `write_world_entry` and `read_post_history` / `write_post_history` tools with a new draft-based workflow. The new flow introduces ephemeral draft files on disk (`/tmp/eni-sidecar/`) that the agent creates, edits, and finalizes. Drafts are previewed in real-time via WebSocket events, and finalization persists the content to SillyTavern's character card fields. The frontend World and Post-History tabs are updated to render a single monolithic text block instead of arrays of cards.

## Glossary

- **Sidecar**: The ENI sidecar Rust backend process that hosts the agent loop, tools, and WebSocket server.
- **Draft_File**: A temporary text file stored at a fixed path under `/tmp/eni-sidecar/` representing in-progress content before finalization.
- **World_Draft**: The draft file at `/tmp/eni-sidecar/world_draft.txt` containing world info text being composed.
- **Post_History_Draft**: The draft file at `/tmp/eni-sidecar/post_history_draft.txt` containing post-history instructions being composed.
- **Preview_Event**: A WebSocket event of type `Preview { tab, data }` sent from the Sidecar to the frontend to display draft content in real-time.
- **StClient**: The HTTP client module that communicates with SillyTavern's REST API for character reads and writes.
- **Session_Context**: Runtime state tracking the last character the agent interacted with (read or wrote), used to infer the finalization target.
- **Tool_Trait**: The Rust trait (`Tool`) that all sidecar tools implement, providing `name()`, `description()`, `parameters_schema()`, `validate_args()`, and `execute()` methods.
- **Finalization**: The act of reading a draft file, writing its content to the appropriate SillyTavern character field via StClient, and deleting the draft file.

## Requirements

### Requirement 1: World Draft Creation

**User Story:** As an agent, I want to create a world info draft file so that I can compose world information incrementally before committing it to the character card.

#### Acceptance Criteria

1. WHEN the `create_world_draft` tool is invoked with a `content` parameter, THE Sidecar SHALL write the content to `/tmp/eni-sidecar/world_draft.txt`.
2. WHEN the `create_world_draft` tool writes the draft file successfully, THE Sidecar SHALL send a Preview_Event with `tab` set to `"world"` and `data` containing the draft text.
3. WHEN the `create_world_draft` tool is invoked and a World_Draft already exists at the fixed path, THE Sidecar SHALL overwrite the existing file with the new content and include a warning message in the tool response indicating the previous draft was replaced.
4. IF the `/tmp/eni-sidecar/` directory does not exist when `create_world_draft` is invoked, THEN THE Sidecar SHALL create the directory before writing the file.
5. WHEN the `create_world_draft` tool completes successfully, THE Sidecar SHALL return a JSON response containing `success: true` and the file path of the created draft.

### Requirement 2: World Draft Editing

**User Story:** As an agent, I want to edit an existing world info draft using text replacement so that I can refine the content iteratively without rewriting the entire draft.

#### Acceptance Criteria

1. WHEN the `edit_world_draft` tool is invoked with `old_text` and `new_text` parameters, THE Sidecar SHALL read the current World_Draft, replace the first occurrence of `old_text` with `new_text`, and write the result back to the draft file.
2. WHEN the `edit_world_draft` tool successfully modifies the draft, THE Sidecar SHALL send a Preview_Event with `tab` set to `"world"` and `data` containing the updated draft text.
3. IF the `edit_world_draft` tool is invoked and no World_Draft exists at the fixed path, THEN THE Sidecar SHALL return an error indicating no draft exists to edit.
4. IF the `old_text` parameter value is not found in the current draft content, THEN THE Sidecar SHALL return an error indicating the text was not found.

### Requirement 3: World Info Finalization

**User Story:** As an agent, I want to finalize the world info draft so that the composed text is prepended to the character's description field in SillyTavern.

#### Acceptance Criteria

1. WHEN the `finalize_world_info` tool is invoked, THE Sidecar SHALL read the World_Draft content from `/tmp/eni-sidecar/world_draft.txt`.
2. WHEN the `finalize_world_info` tool has read the draft, THE Sidecar SHALL determine the target character by reading the avatar URL from Session_Context (the last character the agent read or wrote).
3. WHEN the target character is determined, THE Sidecar SHALL fetch the character's current `description` field via StClient, prepend the draft content followed by a double newline separator (`\n\n`) to the existing description, and write the merged result back via `StClient::edit_character`.
4. WHEN the `finalize_world_info` tool successfully writes to SillyTavern, THE Sidecar SHALL delete the World_Draft file from disk.
5. IF the `finalize_world_info` tool is invoked and no World_Draft exists, THEN THE Sidecar SHALL return an error indicating no draft exists to finalize.
6. IF the Session_Context does not contain a target character (no prior character interaction in the session), THEN THE Sidecar SHALL return an error indicating the target character could not be inferred.
7. WHEN the `finalize_world_info` tool completes successfully, THE Sidecar SHALL send a Preview_Event with `tab` set to `"world"` and `data` set to `null` to clear the preview pane.

### Requirement 4: Post-History Draft Creation

**User Story:** As an agent, I want to create a post-history instructions draft file so that I can compose post-history content incrementally before committing it.

#### Acceptance Criteria

1. WHEN the `create_post_history_draft` tool is invoked with a `content` parameter, THE Sidecar SHALL write the content to `/tmp/eni-sidecar/post_history_draft.txt`.
2. WHEN the `create_post_history_draft` tool writes the draft file successfully, THE Sidecar SHALL send a Preview_Event with `tab` set to `"posthistory"` and `data` containing the draft text.
3. WHEN the `create_post_history_draft` tool is invoked and a Post_History_Draft already exists at the fixed path, THE Sidecar SHALL overwrite the existing file with the new content and include a warning message in the tool response indicating the previous draft was replaced.
4. IF the `/tmp/eni-sidecar/` directory does not exist when `create_post_history_draft` is invoked, THEN THE Sidecar SHALL create the directory before writing the file.
5. WHEN the `create_post_history_draft` tool completes successfully, THE Sidecar SHALL return a JSON response containing `success: true` and the file path of the created draft.

### Requirement 5: Post-History Draft Editing

**User Story:** As an agent, I want to edit an existing post-history draft using text replacement so that I can refine the content iteratively.

#### Acceptance Criteria

1. WHEN the `edit_post_history_draft` tool is invoked with `old_text` and `new_text` parameters, THE Sidecar SHALL read the current Post_History_Draft, replace the first occurrence of `old_text` with `new_text`, and write the result back to the draft file.
2. WHEN the `edit_post_history_draft` tool successfully modifies the draft, THE Sidecar SHALL send a Preview_Event with `tab` set to `"posthistory"` and `data` containing the updated draft text.
3. IF the `edit_post_history_draft` tool is invoked and no Post_History_Draft exists at the fixed path, THEN THE Sidecar SHALL return an error indicating no draft exists to edit.
4. IF the `old_text` parameter value is not found in the current draft content, THEN THE Sidecar SHALL return an error indicating the text was not found.

### Requirement 6: Post-History Finalization

**User Story:** As an agent, I want to finalize the post-history draft so that the composed text is written to the character's `post_history_instructions` field in SillyTavern.

#### Acceptance Criteria

1. WHEN the `finalize_post_history` tool is invoked, THE Sidecar SHALL read the Post_History_Draft content from `/tmp/eni-sidecar/post_history_draft.txt`.
2. WHEN the `finalize_post_history` tool has read the draft, THE Sidecar SHALL determine the target character by reading the avatar URL from Session_Context (the last character the agent read or wrote).
3. WHEN the target character is determined, THE Sidecar SHALL write the draft content to the character's `post_history_instructions` field via `StClient::edit_character`, replacing the existing value entirely.
4. WHEN the `finalize_post_history` tool successfully writes to SillyTavern, THE Sidecar SHALL delete the Post_History_Draft file from disk.
5. IF the `finalize_post_history` tool is invoked and no Post_History_Draft exists, THEN THE Sidecar SHALL return an error indicating no draft exists to finalize.
6. IF the Session_Context does not contain a target character (no prior character interaction in the session), THEN THE Sidecar SHALL return an error indicating the target character could not be inferred.
7. WHEN the `finalize_post_history` tool completes successfully, THE Sidecar SHALL send a Preview_Event with `tab` set to `"posthistory"` and `data` set to `null` to clear the preview pane.

### Requirement 7: Old Tool Removal

**User Story:** As a maintainer, I want the old world entry and post-history tools removed so that the codebase has a single, consistent draft-based workflow.

#### Acceptance Criteria

1. THE Sidecar SHALL remove the `ReadWorldEntriesTool` and `WriteWorldEntryTool` implementations from the tools module.
2. THE Sidecar SHALL remove the `ReadPostHistoryTool` and `WritePostHistoryTool` implementations from the tools module.
3. THE Sidecar SHALL remove the `world_entries.rs` and `post_history.rs` source files.
4. THE Sidecar SHALL remove all registrations of the old tools from the tool dispatcher setup.
5. THE Sidecar SHALL register the six new draft tools (`create_world_draft`, `edit_world_draft`, `finalize_world_info`, `create_post_history_draft`, `edit_post_history_draft`, `finalize_post_history`) with the tool dispatcher.

### Requirement 8: Frontend World Tab Update

**User Story:** As a user, I want the World tab to display a single monolithic text block so that I can see the full world info draft as the agent composes it.

#### Acceptance Criteria

1. WHEN the World tab receives a Preview_Event with `tab` set to `"world"` and `data` containing a string value, THE Frontend SHALL render the string as a single pre-formatted text block.
2. WHEN the World tab receives a Preview_Event with `tab` set to `"world"` and `data` set to `null`, THE Frontend SHALL display the empty-state placeholder.
3. THE Frontend SHALL remove the array-of-cards rendering logic from the World tab component.

### Requirement 9: Frontend Post-History Tab Update

**User Story:** As a user, I want the Post-History tab to display a single monolithic text block so that I can see the full post-history draft as the agent composes it.

#### Acceptance Criteria

1. WHEN the Post-History tab receives a Preview_Event with `tab` set to `"posthistory"` and `data` containing a string value, THE Frontend SHALL render the string as a single pre-formatted text block.
2. WHEN the Post-History tab receives a Preview_Event with `tab` set to `"posthistory"` and `data` set to `null`, THE Frontend SHALL display the empty-state placeholder.
3. THE Frontend SHALL remove the structured fields rendering logic (narration_style, formatting_rules, tone_keywords) from the Post-History tab component.

### Requirement 10: Tool Trait Conformance

**User Story:** As a developer, I want all new draft tools to conform to the existing Tool trait pattern so that they integrate seamlessly with the dispatcher and LLM function-calling interface.

#### Acceptance Criteria

1. THE Sidecar SHALL implement the `Tool` trait for each of the six new tools, providing `name()`, `description()`, `parameters_schema()`, `validate_args()`, and `execute()` methods.
2. THE Sidecar SHALL return a valid JSON Schema from `parameters_schema()` for each tool that accurately describes the tool's accepted parameters.
3. THE Sidecar SHALL validate all required parameters in `validate_args()` before execution proceeds.
