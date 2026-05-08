# Design Document: UI Mockup Alignment

## Overview

This is a styling-only change that migrates the frontend color palette from purple (#7c5cfc) to amber/gold (#e8a33d), centralizes CSS variables into a single `:root` block, and adjusts layout properties on tool cards and thinking blocks to match the HTML mockup.

No new components, data models, or logic changes are required.

## Architecture

The change touches the CSS layer only. A centralized `:root` block in `App.svelte` becomes the single source of truth for all design tokens. Individual components then reference these variables without fallback values.

```
App.svelte (:root variables)
  └── All components inherit tokens via CSS custom properties
```

## Components Modified

### 1. App.svelte — Centralized `:root` Block

Add a `:global(:root)` block inside the `<style>` section defining all theme tokens:

```svelte
<style>
  :global(:root) {
    --accent: #e8a33d;
    --accent-hover: #f0b856;
    --accent-muted: rgba(232, 163, 61, 0.12);
    --accent-border: rgba(232, 163, 61, 0.35);
    --bg-deep: #1b1b1b;
    --bg-surface: #262626;
    --bg-elevated: #2b2b2b;
    --surface-hover: #333333;
    --border: #3e3e3e;
    --text: #cccccc;
    --text-secondary: #999999;
    --text-muted: #666666;
    --success: #4caf50;
    --warning: #ff9800;
    --error: #f44336;
    --radius: 6px;
    --radius-sm: 4px;
    --font: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    --mono: 'JetBrains Mono', 'Fira Code', monospace;
  }

  /* existing .wb-root styles ... */
</style>
```

### 2. ChatPane.svelte — Send Button + Avatar Text Color

```css
.send-btn {
  color: #1b1b1b;  /* was: white */
}

.eni-avatar {
  color: #1b1b1b;  /* was: white */
}
```

### 3. ToolCallCard.svelte — align-self + Icon Background

```css
.tool-card {
  align-self: stretch;  /* was: flex-start */
}

.tool-icon {
  background: rgba(232, 163, 61, 0.15);  /* was: rgba(124, 92, 252, 0.15) */
}
```

### 4. ThinkingBlock.svelte — align-self + Remove max-width

```css
.thinking-block {
  align-self: stretch;  /* was: flex-start */
  /* max-width: 90% removed */
}
```

### 5. MessageBubble.svelte — User Bubble Styling

```css
.message.user .msg-bubble {
  background: var(--accent-muted);       /* was: rgba(124, 92, 252, 0.12) */
  border: 1px solid var(--accent-border); /* was: rgba(124, 92, 252, 0.3) */
}
```

### 6. Tab Components — Tag Pill Colors

In `CharacterTab.svelte`, `PersonaTab.svelte`, `WorldTab.svelte`, `PostHistoryTab.svelte`:

```css
.tag-pill {
  background: rgba(232, 163, 61, 0.1);  /* was: rgba(124, 92, 252, 0.1) */
}
```

In `SettingsTab.svelte` (upload button):

```css
.upload-btn {
  background: rgba(232, 163, 61, 0.1);  /* was: rgba(124, 92, 252, 0.1) */
}
.upload-btn:hover {
  background: rgba(232, 163, 61, 0.2);  /* was: rgba(124, 92, 252, 0.2) */
}
```

### 7. Fallback Removal

After the `:root` block is in place, remove per-component fallback values (e.g., `var(--accent, #7c5cfc)` becomes `var(--accent)`) from all modified components. This keeps the CSS DRY and ensures the central `:root` is the single source of truth.

## Data Models

No data model changes.

## Error Handling

No error handling changes — this is purely cosmetic.

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system — essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

No property-based tests are applicable for this feature. All acceptance criteria are concrete CSS value checks with no meaningful input variation. They are best validated with example-based unit tests or visual regression snapshots:

- Verify the `:root` block contains all expected variables with correct values.
- Verify specific components use the updated color/layout values.
- Verify no component files retain old purple fallback values.

These are deterministic, finite checks — running them 100 times with random inputs would not find additional bugs beyond what a single assertion confirms.
