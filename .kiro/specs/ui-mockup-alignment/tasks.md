# Implementation Plan: UI Mockup Alignment

## Overview

Migrate the frontend color palette from purple to amber/gold and align component layouts with the HTML mockup. All changes are CSS-only — no logic or data model modifications.

## Tasks

- [x] 1. Centralize theme variables and update accent palette
  - [x] 1.1 Add `:global(:root)` block to `frontend/src/App.svelte` with all design tokens (amber/gold palette, neutral backgrounds, text colors, radii, fonts)
    - Define --accent, --accent-hover, --accent-muted, --accent-border, --bg-deep, --bg-surface, --bg-elevated, --surface-hover, --border, --text, --text-secondary, --text-muted, --success, --warning, --error, --radius, --radius-sm, --font, --mono
    - _Requirements: 1.1, 1.2, 1.3, 1.4_
  - [x] 1.2 Remove per-component CSS fallback values from all modified components (replace `var(--accent, #7c5cfc)` patterns with `var(--accent)`)
    - Applies to: ChatPane.svelte, MessageBubble.svelte, ToolCallCard.svelte, ThinkingBlock.svelte, and tab components
    - _Requirements: 1.5_

- [x] 2. Update component layouts and colors to match mockup
  - [x] 2.1 Update `ChatPane.svelte` — set send button and `.eni-avatar` text color to `#1b1b1b`
    - _Requirements: 3.1, 3.2_
  - [x] 2.2 Update `ToolCallCard.svelte` — add `align-self: stretch` and change tool icon background to `rgba(232, 163, 61, 0.15)`
    - _Requirements: 2.1, 5.1_
  - [x] 2.3 Update `ThinkingBlock.svelte` — add `align-self: stretch` and remove `max-width: 90%`
    - _Requirements: 2.2, 2.3_
  - [x] 2.4 Update `MessageBubble.svelte` — set user bubble background to `var(--accent-muted)` and border to `1px solid var(--accent-border)`
    - _Requirements: 4.1, 4.2_
  - [x] 2.5 Update tag pill colors in `CharacterTab.svelte`, `PersonaTab.svelte`, `WorldTab.svelte`, `PostHistoryTab.svelte`, and `SettingsTab.svelte` upload button to use amber/gold values
    - Tag pill background: `rgba(232, 163, 61, 0.1)`
    - Upload button: `rgba(232, 163, 61, 0.1)` / hover: `rgba(232, 163, 61, 0.2)`
    - _Requirements: 5.2_

- [x] 3. Final checkpoint
  - Ensure the app builds without errors, ask the user if questions arise.

## Notes

- This is a styling-only change — no logic, data models, or new components are introduced
- All acceptance criteria are deterministic CSS value checks; no property-based tests are applicable
- The mockup reference is at `mockups/eni-ui-mockup.html`
- Each task references specific requirements for traceability

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1"] },
    { "id": 1, "tasks": ["1.2", "2.1", "2.2", "2.3", "2.4", "2.5"] }
  ]
}
```
