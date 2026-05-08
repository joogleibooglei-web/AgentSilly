# Implementation Plan: SillyTavern World Builder

## Overview

This plan implements the SillyTavern World Builder as a bundled React extension using Webpack. Tasks are ordered so foundational infrastructure (project scaffold, types, stores, persistence) comes first, followed by core services (agent orchestrator, tool executor, diff/schema services), then UI components (panel, chat, structured views), and finally integration, export, and the optional server plugin.

All code is TypeScript. Testing uses Vitest + fast-check for property-based tests.

## Tasks

- [x] 1. Project scaffold and build configuration
  - [x] 1.1 Initialize the extension project using ST's React template with Webpack
    - Create directory structure: `src/`, `src/components/`, `src/stores/`, `src/services/`, `src/tools/`, `src/types/`, `tests/`
    - Configure Webpack for TypeScript + React bundling
    - Add `manifest.json` with extension metadata
    - _Requirements: 1.1, 1.4_

  - [x] 1.2 Configure TailwindCSS with prefix scoping
    - Install TailwindCSS v4 and configure with `wb-` prefix
    - Set up dark color palette variables matching ST's default dark theme
    - Create root container class for style isolation
    - _Requirements: 16.1, 16.2, 16.3_

  - [x] 1.3 Install and configure core dependencies
    - Install: zustand, immer, zod, nanoid, jsondiffpatch, localforage, react-arborist, @codemirror/lang-json, @codemirror/view, @codemirror/state
    - Install shadcn/ui components: Dialog, Tabs, Accordion, Popover, Button, Input, Textarea, ScrollArea, Collapsible, DropdownMenu, Toast
    - Install dev dependencies: vitest, fast-check, fake-indexeddb, @testing-library/react
    - Configure `tsconfig.json` with strict mode
    - _Requirements: 1.1, 16.5_

  - [x] 1.4 Set up Vitest configuration and test infrastructure
    - Create `vitest.config.ts` with fast-check global config (numRuns: 100)
    - Create test directory structure: `tests/property/`, `tests/property/generators/`, `tests/unit/`, `tests/integration/`, `tests/mocks/`
    - Create shared mocks: `st-context.mock.ts`, `generate-raw.mock.ts`, `indexeddb.mock.ts`
    - _Requirements: 15.1_

- [x] 2. Type definitions and data models
  - [x] 2.1 Define core data model types
    - Create `src/types/project.ts`: Project, ProjectPlan, ProjectSettings, Task, TaskStatus, TaskDefinition interfaces
    - Create `src/types/world-document.ts`: WorldDocument, WorldDomain, WorldNode interfaces
    - Create `src/types/character.ts`: CharacterDraft, IdentityComponent, PsychologyComponent, RoleComponent, RelationshipComponent, BehaviorComponent, ContextComponent, RomanceComponent interfaces
    - Create `src/types/post-history.ts`: PostHistoryConfig, PostHistoryTile, HeaderTileConfig, BodyTileConfig, LengthTileConfig, EndingsTileConfig, FooterTileConfig, TimelineTileConfig, VariableDefinition interfaces
    - Create `src/types/conversation.ts`: ConversationMessage, ToolCall, ToolResult, ToolDefinition, ToolCallCardProps interfaces
    - Create `src/types/export.ts`: TavernCardV2Export, CharacterBook interfaces
    - _Requirements: 4.1, 4.2, 4.3, 5.1, 6.1, 11.1_

  - [x] 2.2 Define Zod validation schemas for all data models
    - Create `src/schemas/project.schema.ts`: Zod schemas for Project, Task, TaskStatus transitions
    - Create `src/schemas/world-document.schema.ts`: Zod schemas for WorldDocument, WorldDomain, WorldNode
    - Create `src/schemas/character.schema.ts`: Zod schemas for CharacterDraft and all component types, enforcing MBTI + Enneagram required in psychology
    - Create `src/schemas/post-history.schema.ts`: Zod schemas for PostHistoryConfig and all tile configs
    - Create `src/schemas/conversation.schema.ts`: Zod schemas for ConversationMessage, ToolCall
    - _Requirements: 4.4, 4.5, 7.5_

  - [x] 2.3 Write property test for schema generation correctness (Property 3)
    - **Property 3: Schema Generation Correctness**
    - **Validates: Requirements 4.4, 4.5**
    - Create `tests/property/generators/component-selection.gen.ts` with arbitrary component selections
    - Create `tests/property/schema.property.test.ts` verifying generated schemas match selections exactly

- [x] 3. Zustand stores and state management
  - [x] 3.1 Implement ProjectStore
    - Create `src/stores/project-store.ts` with Zustand + Immer middleware
    - Implement state: projects array, activeProjectId
    - Implement actions: createProject, loadProject, deleteProject, updateProject, addTask, updateTaskStatus, getTaskDrafts, setTaskDraft
    - Implement task state machine validation (planned → in_progress → complete/needs_revision)
    - _Requirements: 2.1, 2.2, 5.1, 5.2_

  - [x] 3.2 Write property test for task state machine (Property 4)
    - **Property 4: Task State Machine Validity**
    - **Validates: Requirements 5.2**
    - Create `tests/property/generators/project.gen.ts` with arbitrary task states
    - Create `tests/property/task-state.property.test.ts` verifying only valid transitions are permitted

  - [x] 3.3 Write property test for dependency graph invariants (Property 5)
    - **Property 5: Dependency Graph Invariants**
    - **Validates: Requirements 5.5, 10.4**
    - Create `tests/property/dependency.property.test.ts` verifying tasks can't start before deps complete

  - [x] 3.4 Implement WorldDocumentStore
    - Create `src/stores/world-document-store.ts` with Zustand + Immer middleware
    - Implement state: document, characters, postHistory, undoStack, redoStack
    - Implement actions: updateSection, addNode, removeNode, moveNode
    - Implement character actions: updateCharacter, importCharacter
    - Implement undo/redo using jsondiffpatch deltas
    - Implement post-history actions: updatePostHistoryTile, togglePostHistoryTile
    - _Requirements: 6.5, 6.6, 8.3, 8.4, 12.2, 15.3, 15.4_

  - [x] 3.5 Write property test for world document tree mutations (Property 7)
    - **Property 7: World Document Tree Mutations**
    - **Validates: Requirements 6.6, 8.3, 8.4**
    - Create `tests/property/generators/world-document.gen.ts` with arbitrary tree structures
    - Create `tests/property/world-document.property.test.ts` verifying add/remove correctness

  - [x] 3.6 Write property test for undo/redo consistency (Property 15)
    - **Property 15: Undo/Redo Consistency**
    - **Validates: Requirements 15.3, 15.4**
    - Create `tests/property/undo-redo.property.test.ts` verifying N undos restore initial state and N redos restore final state

  - [x] 3.7 Implement UIStore
    - Create `src/stores/ui-store.ts`
    - Implement state: panelOpen, panelWidth, activeView, selectedTreeNode, selectedTaskId
    - Implement actions: togglePanel, setActiveView, selectTreeNode, selectTask
    - _Requirements: 1.1, 1.2_

  - [x] 3.8 Implement ConversationStore
    - Create `src/stores/conversation-store.ts`
    - Implement state: conversations (Record<string, ConversationMessage[]>), activeConversationId, isGenerating
    - Implement actions: appendMessage, setGenerating, loadConversation, clearConversation
    - _Requirements: 3.3, 3.4_

  - [x] 3.9 Write property test for parallel task isolation (Property 6)
    - **Property 6: Parallel Task Isolation**
    - **Validates: Requirements 5.7**
    - Create `tests/property/parallel.property.test.ts` verifying concurrent mutations on separate tasks don't interfere

- [x] 4. Persistence layer
  - [x] 4.1 Implement PersistenceService
    - Create `src/services/persistence-service.ts`
    - Implement save, load, remove, listKeys, getStorageStatus using localforage (IndexedDB)
    - Implement fallback chain: IndexedDB → localStorage (metadata only) → in-memory
    - Implement debounced auto-save (2-second delay after mutations)
    - Implement storage key schema: `project:{id}`, `world:{projectId}`, `conversation:{taskId}`, `draft:{taskId}`, `diff-history:{projectId}`, `metadata`
    - _Requirements: 2.3, 15.1, 15.2, 15.5_

  - [x] 4.2 Implement store persistence middleware
    - Create Zustand middleware that subscribes to store changes and triggers PersistenceService.save
    - Implement state restoration on extension load (hydrate stores from IndexedDB)
    - Implement data migration for schema version mismatches
    - _Requirements: 2.2, 15.1, 15.2_

  - [x] 4.3 Write property test for persistence round-trip (Property 1)
    - **Property 1: Persistence Round-Trip**
    - **Validates: Requirements 2.2, 2.3, 3.3, 15.2**
    - Create `tests/property/persistence.property.test.ts` using fake-indexeddb
    - Verify serialize → deserialize produces equivalent state for arbitrary project states

  - [x] 4.4 Write property test for project deletion completeness (Property 2)
    - **Property 2: Project Deletion Completeness**
    - **Validates: Requirements 2.4**
    - Create deletion completeness test in `tests/property/persistence.property.test.ts`
    - Verify zero keys remain referencing deleted project ID

- [x] 5. Checkpoint - Core infrastructure
  - Ensure all tests pass, ask the user if questions arise.

- [x] 6. Core services - DiffService and SchemaGenerator
  - [x] 6.1 Implement DiffService
    - Create `src/services/diff-service.ts`
    - Implement diff, patch, unpatch using jsondiffpatch
    - Implement formatDiff for human-readable change summaries
    - Configure objectHash for array diffing on entities with IDs
    - _Requirements: 12.4, 12.5, 15.3_

  - [x] 6.2 Write property test for diff patch round-trip (Property 13)
    - **Property 13: Diff Patch Round-Trip**
    - **Validates: Requirements 12.4**
    - Create `tests/property/generators/character.gen.ts` with arbitrary character states
    - Create `tests/property/diff.property.test.ts` verifying diff → patch produces modified state and unpatch produces original

  - [x] 6.3 Write property test for minimal field write (Property 14)
    - **Property 14: Minimal Field Write**
    - **Validates: Requirements 12.5**
    - Add test to `tests/property/diff.property.test.ts` verifying write payload contains only changed fields

  - [x] 6.4 Implement SchemaGenerator
    - Create `src/services/schema-generator.ts`
    - Implement generateSchema: takes ComponentSelection, returns JSON Schema with only selected component properties
    - Implement validate: validates data against generated schema using Zod
    - Enforce psychology component always requires MBTI + Enneagram when selected
    - Define component vocabularies for character, world, and post-history types
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5_

- [x] 7. Core services - AgentService and ToolService
  - [x] 7.1 Implement ToolService
    - Create `src/services/tool-service.ts`
    - Implement executeTool dispatcher routing to appropriate handlers
    - Implement getToolDefinitions returning JSON Schema for all 7 tools
    - Implement tool handlers: read_character (ST API), write_character (ST API), create_character (ST API), get_world_state (WorldDocumentStore), update_world_state (WorldDocumentStore), search_wiki (server plugin proxy), get_wiki_page (server plugin proxy)
    - Implement graceful degradation when server plugin unavailable for wiki tools
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 6.7, 6.8, 6.9_

  - [x] 7.2 Implement AgentService
    - Create `src/services/agent-service.ts`
    - Implement initializeSession: load task context (description, deps drafts, component vocabulary)
    - Implement sendMessage: assemble prompt layers → call generateRaw → parse response → execute tool calls → return final response
    - Implement cancelGeneration using AbortController
    - Implement getAssembledPrompt for debugging/preview
    - Implement prompt assembly order: system → post-instructions → task context → history → tools
    - Implement conversation history truncation when exceeding context window (oldest messages first)
    - Implement structured output parsing with lenient fallback (strip markdown fences, fix common JSON errors)
    - _Requirements: 3.1, 3.5, 3.6, 5.3, 13.1, 13.2, 13.4, 13.5_

  - [x] 7.3 Write property test for prompt assembly correctness (Property 9)
    - **Property 9: Prompt Assembly Correctness**
    - **Validates: Requirements 3.1, 13.1, 13.2, 13.5**
    - Create `tests/property/generators/prompt.gen.ts` with arbitrary prompt layers and conversation lengths
    - Create `tests/property/prompt.property.test.ts` verifying layer order, content inclusion, and truncation behavior

- [x] 8. Export and import services
  - [x] 8.1 Implement character export (TavernCard V2 assembly)
    - Create `src/services/export-service.ts`
    - Implement assembleCharacterCard: map components to V2 fields (world → description, system prompt → system_prompt, post-history → post_history_instructions)
    - Implement token budget validation: check each field against configured budget, return warnings for over-budget fields
    - Implement export as JSON file download
    - Implement apply-to-character via ST character edit API
    - Implement create-new-character via ST character creation API
    - _Requirements: 11.1, 11.2, 11.3, 11.4, 11.5_

  - [x] 8.2 Write property test for character card export conformance (Property 10)
    - **Property 10: Character Card Export Conformance**
    - **Validates: Requirements 11.1**
    - Create `tests/property/export.property.test.ts` verifying assembled cards conform to TavernCard V2 schema

  - [x] 8.3 Write property test for token budget validation (Property 11)
    - **Property 11: Token Budget Validation**
    - **Validates: Requirements 11.5**
    - Add test to `tests/property/export.property.test.ts` verifying warnings identify exactly the over-budget fields

  - [x] 8.4 Implement character import (parse existing V2 cards into components)
    - Create `src/services/import-service.ts`
    - Implement parseCharacterToComponents: parse TavernCard V2 into Component_Vocabulary structure
    - Implement change tracking using DiffService for imported characters
    - Implement minimal field write: compute diff and write only changed fields to ST API
    - _Requirements: 12.1, 12.2, 12.3, 12.4, 12.5_

  - [x] 8.5 Write property test for character import parsing (Property 12)
    - **Property 12: Character Import Parsing**
    - **Validates: Requirements 12.2**
    - Create `tests/property/generators/character.gen.ts` with arbitrary valid TavernCard V2 structures
    - Create `tests/property/import.property.test.ts` verifying parse → reassemble preserves semantic content

- [x] 9. Post-history assembly service
  - [x] 9.1 Implement post-history tile assembly
    - Create `src/services/post-history-service.ts`
    - Implement assembleTiles: iterate enabled tiles in order, concatenate their rendered output
    - Implement per-tile renderers: header (template interpolation), body, length rules, endings, footer, timeline events
    - Implement live preview generation (called on any tile config change)
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.5_

  - [x] 9.2 Write property test for post-history assembly composition (Property 8)
    - **Property 8: Post-History Assembly Composition**
    - **Validates: Requirements 9.2**
    - Create `tests/property/generators/post-history.gen.ts` with arbitrary tile configurations
    - Create `tests/property/post-history.property.test.ts` verifying output contains content from all and only enabled tiles in order

- [x] 10. Checkpoint - Core services complete
  - Ensure all tests pass, ask the user if questions arise.

- [x] 11. UI - Extension panel and layout
  - [x] 11.1 Implement ExtensionPanel root component
    - Create `src/components/ExtensionPanel.tsx`
    - Implement slide-out panel (40-60% screen width) with resize handle
    - Implement two-pane layout: left (chat), right (structured view)
    - Wrap in style isolation container with TailwindCSS prefix scope
    - Register panel toggle in ST extensions menu
    - _Requirements: 1.1, 1.2, 16.1, 16.2, 16.3_

  - [x] 11.2 Implement theme adaptation
    - Create `src/styles/theme.ts` with CSS variable mappings
    - Listen for ST theme change events and update CSS variables
    - Implement dark color palette consistent with ST's default theme
    - _Requirements: 16.3, 16.4_

  - [x] 11.3 Implement view routing
    - Create `src/components/ViewRouter.tsx`
    - Route between views based on UIStore.activeView: project, character, world, post_history
    - Implement view switching via tabs or task selection
    - _Requirements: 1.2, 10.2_

- [x] 12. UI - ENI Chat pane
  - [x] 12.1 Implement ChatPane component
    - Create `src/components/chat/ChatPane.tsx`
    - Implement message list with scroll-to-bottom behavior
    - Implement message input with send button and keyboard shortcut (Enter to send)
    - Implement loading indicator during generation
    - Implement disable send button while generating
    - Implement retry button on failed messages
    - _Requirements: 3.1, 3.2, 3.5, 3.6_

  - [x] 12.2 Implement message rendering
    - Create `src/components/chat/MessageBubble.tsx`
    - Render user messages and ENI responses with markdown support
    - Implement inline collapsible JSON previews
    - Implement formatted character previews
    - _Requirements: 3.2_

  - [x] 12.3 Implement tool call display cards
    - Create `src/components/chat/ToolCallCard.tsx`
    - Render compact styled cards for tool calls with icon, summary, status indicator
    - Implement expandable detail section showing parameters and results
    - Generate human-readable summaries per tool type (search_wiki → "🔍 Searched wiki: {wiki} for '{query}'", etc.)
    - _Requirements: 6.1, 6.9_

  - [x] 12.4 Wire ChatPane to AgentService and ConversationStore
    - Connect send button to AgentService.sendMessage
    - Subscribe to ConversationStore for message updates
    - Implement task switching: load correct conversation when selectedTaskId changes
    - Implement cancel generation button wired to AgentService.cancelGeneration
    - _Requirements: 3.1, 3.3, 3.4_

- [x] 13. UI - Project/Task Board view
  - [x] 13.1 Implement ProjectBoardView
    - Create `src/components/views/ProjectBoardView.tsx`
    - Display project plan at top (editable text area)
    - Display task list with status indicators (color-coded: planned, in_progress, complete, needs_revision)
    - Implement visual dependency indicators between tasks
    - Implement "New Project" button and project selector dropdown
    - _Requirements: 10.1, 10.3, 10.5_

  - [x] 13.2 Implement task interaction
    - Implement click-to-open task (loads task in chat pane with context)
    - Implement "Start Task" button with dependency check (warn if deps incomplete)
    - Implement task status transitions via UI controls
    - Implement add/remove tasks from the board
    - _Requirements: 5.2, 5.3, 5.5, 10.2, 10.4, 10.5_

  - [x] 13.3 Implement project deletion with confirmation
    - Implement delete project button with confirmation dialog
    - Wire to ProjectStore.deleteProject which removes all associated data from IndexedDB
    - _Requirements: 2.4_

- [x] 14. UI - Character View
  - [x] 14.1 Implement CharacterView component
    - Create `src/components/views/CharacterView.tsx`
    - Display collapsible sections for each selected component (Identity, Psychology, Relationships, Behavior, Context, Romance)
    - Display MBTI and Enneagram prominently at top of Psychology section
    - Implement inline field editing with immediate store updates
    - Implement inline validation errors for malformed fields (Zod validation)
    - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5_

  - [x] 14.2 Implement component selection UI
    - Create `src/components/views/ComponentSelector.tsx`
    - Display Component_Vocabulary as selectable checkboxes/toggles for character, world, and post-history task types
    - Wire selection confirmation to SchemaGenerator.generateSchema
    - _Requirements: 4.1, 4.2, 4.3, 4.5_

  - [x] 14.3 Implement character import flow
    - Create `src/components/views/CharacterImportDialog.tsx`
    - Display list of characters from ST library
    - On selection, parse into components via ImportService and load into store
    - Display diff view before applying changes back to imported character
    - _Requirements: 12.1, 12.2, 12.3, 12.4_

- [x] 15. UI - World View
  - [x] 15.1 Implement WorldView with tree navigation
    - Create `src/components/views/WorldView.tsx`
    - Implement tree navigation on left using react-arborist (virtualized, drag-and-drop)
    - Implement content panel on right showing selected node's data with editable fields
    - Wire tree selection to UIStore.selectedTreeNode
    - _Requirements: 8.1, 8.2_

  - [x] 15.2 Implement world tree CRUD operations
    - Implement add domain/entry via tree context menu or button
    - Implement remove node with confirmation dialog
    - Implement inline rename in tree
    - Wire all operations to WorldDocumentStore actions
    - Commit and push to main (emphasis on main branch), and ask user any questions, being sure to tell them what to expect if it was run at this point in sillytavern
    - _Requirements: 8.3, 8.4, 8.5_

- [x] 16. UI - Post-History View
  - [x] 16.1 Implement PostHistoryView with tile layout
    - Create `src/components/views/PostHistoryView.tsx`
    - Display component tiles for each selected element (Header, Body, Length, Endings, Footer, Timeline)
    - Implement toggle on/off per tile
    - Implement tile configuration editing (per-type config forms)
    - _Requirements: 9.1, 9.2, 9.4_

  - [x] 16.2 Implement live preview panel
    - Create `src/components/views/PostHistoryPreview.tsx`
    - Display assembled post-history output at bottom of view
    - Update preview immediately on any tile config change or toggle
    - Wire to PostHistoryService.assembleTiles
    - _Requirements: 9.3, 9.4_

- [x] 17. UI - Export panel
  - [x] 17.1 Implement ExportPanel
    - Create `src/components/views/ExportPanel.tsx`
    - Display assembled JSON preview showing field-to-ST-field mapping
    - Implement "Export as JSON" button (file download)
    - Implement "Apply to Character" button (write to existing character via ST API)
    - Implement "Create New Character" button (create via ST API)
    - Display token budget warnings for over-budget fields
    - _Requirements: 11.1, 11.2, 11.3, 11.4, 11.5_

- [x] 18. UI - Settings and post-instructions editor
  - [x] 18.1 Implement settings interface
    - Create `src/components/settings/SettingsPanel.tsx`
    - Implement post-instructions editor using CodeMirror 6 (or textarea for v1)
    - Implement project-level settings: wiki URL, token budget
    - Wire post-instructions changes to AgentService (apply to subsequent generateRaw calls without task restart)
    - _Requirements: 13.3, 13.4_

- [x] 19. Checkpoint - UI components complete
  - Ensure all tests pass, ask the user if questions arise.

- [x] 20. Optional server plugin for wiki access
  - [x] 20.1 Implement server plugin scaffold
    - Create `server/` directory with Express router
    - Create `server/index.ts` with plugin registration for ST server plugin system
    - Implement health check endpoint: `GET /api/plugins/world-builder/status`
    - _Requirements: 14.1, 14.4_

  - [x] 20.2 Implement wiki search endpoint
    - Create `server/routes/wiki.ts`
    - Implement `GET /api/plugins/world-builder/wiki/search` using MediaWiki Action API
    - Parse and return structured search results
    - Implement rate limit handling (queue + retry after Retry-After header)
    - _Requirements: 14.1, 14.2, 14.5_

  - [x] 20.3 Implement wiki page retrieval endpoint
    - Implement `GET /api/plugins/world-builder/wiki/page` using MediaWiki Action API + wtf_wikipedia for wikitext parsing
    - Return structured data: infobox fields, sections, links
    - Use cheerio for HTML parsing fallback
    - _Requirements: 14.1, 14.3_

  - [x] 20.4 Implement wiki category listing endpoint
    - Implement `GET /api/plugins/world-builder/wiki/categories` using MediaWiki categorymembers API
    - Return paginated category member lists
    - _Requirements: 14.1_

- [x] 21. Integration wiring and error handling
  - [x] 21.1 Wire extension initialization lifecycle
    - Create `src/index.ts` entry point
    - Implement extension activation: register panel, initialize stores, hydrate from IndexedDB, register ENI system prompt
    - Implement error boundary for missing dependencies (display error message)
    - Listen for ST events: CHARACTER_EDITED (sync imported characters)
    - _Requirements: 1.1, 1.3, 1.4, 1.5, 2.2_

  - [x] 21.2 Implement error handling across all services
    - Implement generateRaw failure handling: display error in chat, preserve message for retry
    - Implement structured output parse failure: lenient parsing → fallback to plain text
    - Implement persistence failure: fallback chain with user warning toast
    - Implement data corruption detection: Zod validation on load, offer recovery dialog
    - Implement server plugin unavailable: graceful degradation for wiki tools
    - _Requirements: 2.5, 3.6, 6.8, 15.5_

  - [x] 21.3 Wire real-time updates between ENI tools and structured views
    - When ENI invokes write_character → update CharacterView in real time
    - When ENI invokes update_world_state → update WorldView tree in real time
    - When ENI generates post-history content → populate tiles and update preview
    - _Requirements: 7.4, 8.5, 9.5_

- [x] 22. Final checkpoint - Full integration
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties from the design document
- Unit tests validate specific examples and edge cases
- The server plugin (task 20) is optional and the extension must function fully without it
- React Flow graph visualization is deferred to a later version per the planning vision
- CodeMirror 6 is included for the settings/post-instructions editor; JSON editing in structured views uses standard form inputs
