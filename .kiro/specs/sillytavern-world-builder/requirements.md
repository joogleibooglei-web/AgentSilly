# Requirements Document

## Introduction

SillyTavern World Builder is a bundled React extension for SillyTavern that provides an agentic workflow for building character cards, world information, and post-history instructions. An AI agent called ENI works with the user through a structured planning → design → implementation process, producing high-quality, composable content for SillyTavern characters. The extension uses a composable format system where users select which components they need, and ENI helps fill them through discrete, token-efficient tasks.

## Glossary

- **Extension**: A client-side JavaScript module that runs in the SillyTavern browser context and extends its functionality
- **ENI**: The AI agent embedded in the extension that assists users in building characters and worlds through conversation
- **Component_Vocabulary**: The set of composable building blocks (identity, psychology, relationships, geography, factions, post-history elements) that ENI can assemble into character cards and world documents
- **Task**: A discrete unit of work within a project (e.g., "Build character: Darlene", "Build world geography") with its own session and context
- **Project**: A collection of tasks, drafts, and world state that together produce one or more character cards and associated world information
- **Character_Card**: A structured data object conforming to the TavernCard V2 specification, containing character description, personality, system prompt, post-history instructions, and extension metadata
- **World_Document**: A structured JSON document containing world information (geography, factions, politics, lore, relationships, timeline) stored in the extension's persistent state
- **Post_History_Instructions**: Mechanical rules injected after chat history that control AI response format (headers, footers, length rules, timeline events, ending conventions)
- **Server_Plugin**: An optional Node.js/Express plugin running on the SillyTavern server that provides MediaWiki API access and web search capabilities
- **Structured_Output**: A JSON response from the LLM that conforms to a provided JSON schema, used for ENI's tool calls and data generation
- **Task_Context**: The per-task prompt payload containing the task description, relevant drafts from other tasks, and component vocabulary for the current task type
- **generateRaw**: A SillyTavern API function that generates LLM text without chat context, giving full control over prompt construction
- **Panel**: The extension's main UI surface that slides out from the right side of the SillyTavern interface
- **Zustand_Store**: The client-side state management layer using Zustand with Immer middleware for immutable updates to world and UI state

## Requirements

### Requirement 1: Extension Initialization and Panel Rendering

**User Story:** As a SillyTavern user, I want to open the World Builder panel from the extensions menu, so that I can access ENI and begin building characters and worlds.

#### Acceptance Criteria

1. WHEN the user activates the World Builder extension, THE Extension SHALL render a slide-out panel on the right side of the SillyTavern interface occupying 40-60% of screen width
2. WHEN the panel is open, THE Extension SHALL display a two-pane layout with an ENI chat pane on the left and a structured view pane on the right
3. THE Extension SHALL initialize the Zustand_Store with default empty state for projects, tasks, characters, and world documents on first load
4. WHEN the extension loads, THE Extension SHALL register ENI's system prompt and post-history instructions in memory for use with generateRaw calls
5. IF the panel fails to render due to a missing dependency, THEN THE Extension SHALL display an error message identifying the missing component

### Requirement 2: Project Management

**User Story:** As a user, I want to create, load, and manage projects, so that I can organize my world-building work into discrete efforts.

#### Acceptance Criteria

1. WHEN the user clicks "New Project", THE Extension SHALL create a new project with a unique identifier, empty task list, and empty world document
2. WHEN the user selects an existing project from the project list, THE Extension SHALL load that project's tasks, drafts, and world state into the Zustand_Store
3. THE Extension SHALL persist all project data to IndexedDB via localforage so that projects survive page refreshes and browser restarts
4. WHEN the user deletes a project, THE Extension SHALL remove all associated tasks, drafts, and world state from persistent storage after user confirmation
5. IF a project fails to load from storage due to data corruption, THEN THE Extension SHALL display an error message and offer to create a new project

### Requirement 3: ENI Chat Interface

**User Story:** As a user, I want to converse with ENI in a chat-like interface, so that I can plan and design my characters and worlds collaboratively.

#### Acceptance Criteria

1. WHEN the user sends a message in the ENI chat pane, THE Extension SHALL call generateRaw with ENI's system prompt, the current task context, and the user's message
2. WHEN ENI generates a response, THE Extension SHALL display it in the chat pane with support for inline collapsible JSON previews and formatted character previews
3. THE Extension SHALL maintain a per-task conversation history that is stored in the Zustand_Store and persisted to IndexedDB
4. WHEN the user switches between tasks, THE Extension SHALL load the conversation history for the selected task into the chat pane
5. WHILE a generateRaw call is in progress, THE Extension SHALL display a loading indicator in the chat pane and disable the send button
6. IF a generateRaw call fails due to a network error or API timeout, THEN THE Extension SHALL display the error message and allow the user to retry the message

### Requirement 4: Composable Format System

**User Story:** As a user, I want to select which components my character or world needs from a vocabulary of building blocks, so that I get exactly the structure I need without unnecessary bloat.

#### Acceptance Criteria

1. WHEN the user starts a new character task, THE Extension SHALL present the Component_Vocabulary for character cards (identity, psychology, role, relationships, behavior, context, romance) as selectable options
2. WHEN the user starts a new world task, THE Extension SHALL present the Component_Vocabulary for world documents (geography, politics, factions, lore, relationships, timeline, entities, culture, economy) as selectable options
3. WHEN the user starts a new post-history task, THE Extension SHALL present the Component_Vocabulary for post-history instructions (header, body, length rules, ending conventions, footer, timeline events, example output) as selectable options
4. THE Extension SHALL enforce that the psychology component always includes MBTI and Enneagram fields when selected
5. WHEN the user confirms their component selection, THE Extension SHALL generate a JSON schema reflecting only the selected components for use in structured output generation

### Requirement 5: Task and Spec System

**User Story:** As a user, I want my work broken into discrete tasks with their own sessions and context, so that each task is focused, token-efficient, and independently iterable.

#### Acceptance Criteria

1. WHEN ENI and the user complete the planning phase, THE Extension SHALL produce a structured plan containing a list of tasks with descriptions, types, and dependency declarations
2. THE Extension SHALL support task states: Planned, In_Progress, Complete, and Needs_Revision
3. WHEN the user clicks "Start Task" on a planned task, THE Extension SHALL create a new ENI session loaded with the task description, ENI's system prompt, post-instructions, relevant drafts from completed tasks, and the component vocabulary for that task type
4. WHEN a task is marked Complete, THE Extension SHALL store its output draft in persistent storage accessible to other tasks via cross-instance access
5. WHEN a task declares dependencies on other tasks, THE Extension SHALL display a warning if the user attempts to start it before its dependencies are Complete
6. WHEN the user marks a task as Needs_Revision, THE Extension SHALL allow re-opening the task session while preserving the existing draft as a starting point
7. THE Extension SHALL support parallel task execution by allowing multiple tasks to be In_Progress simultaneously without shared mutable state conflicts

### Requirement 6: ENI Tool System

**User Story:** As a user, I want ENI to have tools for reading and writing character data, querying wikis, and managing world state, so that ENI can take concrete actions during our collaboration.

#### Acceptance Criteria

1. THE Extension SHALL implement ENI's tool interface with the following tools: search_wiki, get_wiki_page, read_character, write_character, create_character, get_world_state, update_world_state
2. WHEN ENI invokes read_character, THE Extension SHALL retrieve the specified character card from the SillyTavern character library via the ST API
3. WHEN ENI invokes write_character, THE Extension SHALL update the specified fields of an existing character card via the ST character edit API
4. WHEN ENI invokes create_character, THE Extension SHALL create a new character card via the ST character creation API with the provided fields
5. WHEN ENI invokes get_world_state, THE Extension SHALL return the current World_Document from the Zustand_Store for the active project
6. WHEN ENI invokes update_world_state, THE Extension SHALL apply the specified changes to the World_Document in the Zustand_Store and persist the update
7. WHEN ENI invokes search_wiki or get_wiki_page, THE Extension SHALL route the request to the Server_Plugin if available
8. IF the Server_Plugin is not available and ENI invokes search_wiki or get_wiki_page, THEN THE Extension SHALL inform the user that wiki access requires the optional server plugin
9. THE Extension SHALL pass tool definitions as a JSON schema to generateRaw to enable structured output tool calling

### Requirement 7: Structured View - Character View

**User Story:** As a user, I want to see and directly edit my character's data in a structured card layout, so that I can review and refine ENI's output visually.

#### Acceptance Criteria

1. WHEN a character task is active, THE Extension SHALL display the Character View in the structured view pane showing collapsible sections for each selected component (Identity, Psychology, Relationships, Behavior, Context)
2. THE Extension SHALL display MBTI type and Enneagram type prominently at the top of the Psychology section
3. WHEN the user edits a field directly in the Character View, THE Extension SHALL update the corresponding value in the Zustand_Store and persist the change
4. WHEN ENI writes character data via the write_character tool, THE Extension SHALL update the Character View in real time to reflect the changes
5. THE Extension SHALL validate character data against the component schema and display inline validation errors for malformed fields

### Requirement 8: Structured View - World View

**User Story:** As a user, I want to browse and edit my world information in a hierarchical tree view, so that I can navigate complex world structures efficiently.

#### Acceptance Criteria

1. WHEN a world task is active, THE Extension SHALL display the World View with a tree navigation on the left (using react-arborist) and a content panel on the right showing the selected node's data
2. WHEN the user selects a node in the tree, THE Extension SHALL display that node's content in the content panel with editable fields
3. WHEN the user adds a new domain or entry via the tree view, THE Extension SHALL create the corresponding node in the World_Document and update the tree
4. WHEN the user removes a node from the tree, THE Extension SHALL remove the corresponding data from the World_Document after confirmation
5. WHEN ENI invokes update_world_state, THE Extension SHALL update the tree view to reflect structural changes to the World_Document

### Requirement 9: Structured View - Post-History View

**User Story:** As a user, I want to configure and preview my post-history instructions as toggleable component tiles, so that I can see exactly what will be injected after chat history.

#### Acceptance Criteria

1. WHEN a post-history task is active, THE Extension SHALL display the Post-History View with component tiles for each selected element (Header, Body, Length, Endings, Footer, Timeline)
2. WHEN the user toggles a tile off, THE Extension SHALL exclude that component from the assembled post-history output
3. THE Extension SHALL display a live preview at the bottom of the Post-History View showing the fully assembled post-history block as it would appear in the character card
4. WHEN the user edits a tile's configuration, THE Extension SHALL update the live preview immediately
5. WHEN ENI generates post-history content, THE Extension SHALL populate the corresponding tiles and update the preview

### Requirement 10: Structured View - Project/Task Board

**User Story:** As a user, I want a project overview showing my plan and all tasks with their statuses and dependencies, so that I can track progress and navigate between tasks.

#### Acceptance Criteria

1. WHEN the user opens the Project View, THE Extension SHALL display the project plan at the top and a task list below with status indicators (Planned, In_Progress, Complete, Needs_Revision)
2. WHEN the user clicks a task in the task list, THE Extension SHALL open that task in the ENI chat pane with its context loaded
3. THE Extension SHALL display visual dependency indicators between tasks showing which tasks block others
4. WHEN a task transitions to Complete, THE Extension SHALL update the task board and enable any tasks that were blocked by that dependency
5. THE Extension SHALL allow the user to edit the project plan and add or remove tasks from the task board

### Requirement 11: Character Card Export

**User Story:** As a user, I want to export my completed work as a SillyTavern character card or apply it directly to an existing character, so that I can use my creations in roleplay.

#### Acceptance Criteria

1. WHEN the user clicks "Export", THE Extension SHALL assemble the final character card JSON conforming to the TavernCard V2 specification, mapping world data to the description field, ENI's system prompt to the system_prompt field, and post-history instructions to the post_history_instructions field
2. WHEN the user clicks "Apply to Character", THE Extension SHALL write the assembled data to the selected existing character via the ST character edit API
3. WHEN the user clicks "Create New Character", THE Extension SHALL create a new character card via the ST character creation API with the assembled data
4. THE Extension SHALL display a preview of the assembled JSON showing which ST fields each component maps to before export
5. IF the assembled character card exceeds the token budget for any field, THEN THE Extension SHALL display a warning indicating which fields are over budget

### Requirement 12: Editing Existing Characters

**User Story:** As a user, I want to import an existing character from my SillyTavern library for ENI to enhance, so that I can improve characters I already have.

#### Acceptance Criteria

1. WHEN the user selects "Edit Existing Character", THE Extension SHALL present a list of characters from the SillyTavern character library
2. WHEN the user selects a character to edit, THE Extension SHALL parse the character's existing data into the Component_Vocabulary structure and load it into the Zustand_Store
3. WHEN ENI reads an imported character via read_character, THE Extension SHALL provide the full character card data including all V2 fields
4. THE Extension SHALL track changes made to an imported character using jsondiffpatch so the user can review a diff before applying changes
5. WHEN the user applies changes to an imported character, THE Extension SHALL write only the modified fields to the character card via the ST character edit API

### Requirement 13: Prompt Hierarchy and Context Injection

**User Story:** As a user, I want ENI's prompt to follow a clear hierarchy (ENI card → post-instructions → task context), so that ENI behaves consistently while adapting to each task.

#### Acceptance Criteria

1. THE Extension SHALL construct ENI's prompt in the following order: ENI's system prompt (fixed, lowest priority), post-instructions (user-modifiable, highest priority), then task context (per-task)
2. WHEN a task session starts, THE Extension SHALL inject the task description, relevant drafts from dependent tasks, and the component vocabulary for that task type into the task context layer
3. THE Extension SHALL allow the user to view and modify the post-instructions layer through a dedicated settings interface
4. WHEN the user modifies post-instructions, THE Extension SHALL apply the changes to all subsequent generateRaw calls without requiring a task restart
5. THE Extension SHALL limit the total prompt size to fit within the model's context window by truncating older conversation history first

### Requirement 14: Optional Server Plugin for Wiki Access

**User Story:** As a user, I want an optional server plugin that provides MediaWiki API access, so that ENI can pull structured data from fandom wikis to inform world building.

#### Acceptance Criteria

1. WHEN the Server_Plugin is installed and enabled, THE Extension SHALL expose wiki endpoints: search, page retrieval, and category listing via the MediaWiki API
2. WHEN ENI invokes search_wiki with a query, THE Server_Plugin SHALL query the configured wiki's MediaWiki API and return structured search results
3. WHEN ENI invokes get_wiki_page with a page title, THE Server_Plugin SHALL fetch the page content, parse it using wtf_wikipedia, and return structured data including infobox fields, sections, and links
4. THE Extension SHALL function fully without the Server_Plugin installed, with wiki-dependent tools gracefully degraded
5. IF the Server_Plugin encounters a rate limit from the MediaWiki API, THEN THE Server_Plugin SHALL queue the request and retry after the rate limit window expires

### Requirement 15: State Persistence and Recovery

**User Story:** As a user, I want my work to be automatically saved and recoverable, so that I never lose progress due to page refreshes, browser crashes, or session timeouts.

#### Acceptance Criteria

1. THE Extension SHALL auto-save all Zustand_Store state changes to IndexedDB within 2 seconds of any mutation
2. WHEN the extension loads, THE Extension SHALL restore the most recent project state from IndexedDB into the Zustand_Store
3. THE Extension SHALL maintain a history of World_Document changes using jsondiffpatch, enabling undo and redo operations
4. WHEN the user performs an undo operation, THE Extension SHALL revert the World_Document to its previous state using the stored diff
5. IF IndexedDB is unavailable or full, THEN THE Extension SHALL fall back to localStorage for critical project metadata and display a warning about reduced storage capacity

### Requirement 16: UI Styling and Isolation

**User Story:** As a user, I want the World Builder panel to match SillyTavern's dark theme without interfering with the existing UI, so that the extension feels native and causes no visual regressions.

#### Acceptance Criteria

1. THE Extension SHALL scope all TailwindCSS classes with a prefix to prevent style collisions with SillyTavern's existing CSS
2. THE Extension SHALL render all UI within a single container element that isolates its styles from the parent SillyTavern DOM
3. THE Extension SHALL use a dark color palette consistent with SillyTavern's default dark theme
4. WHEN SillyTavern's theme changes, THE Extension SHALL adapt its color variables to remain visually consistent with the active theme
5. THE Extension SHALL use shadcn/ui components built on Radix UI primitives for accessibility compliance (keyboard navigation, ARIA attributes, focus management)

