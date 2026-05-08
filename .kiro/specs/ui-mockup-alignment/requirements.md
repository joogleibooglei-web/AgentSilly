# Requirements Document

## Introduction

Update the Svelte frontend UI to match the HTML mockup in `mockups/eni-ui-mockup.html`. The changes are primarily cosmetic: migrating the color palette from purple to amber/gold, making tool cards and thinking blocks full-width, updating text colors on accent backgrounds, styling user message bubbles, and consolidating scattered CSS variable fallback values into a single centralized `:root` block.

## Glossary

- **Frontend**: The Svelte 5 application located at `frontend/src/`
- **Theme_Variables**: CSS custom properties (design tokens) defined in a centralized `:root` block that control colors, radii, and fonts across all components
- **ToolCallCard**: The Svelte component (`ToolCallCard.svelte`) that displays tool execution status in the chat
- **ThinkingBlock**: The Svelte component (`ThinkingBlock.svelte`) that displays the agent's thinking process
- **MessageBubble**: The Svelte component (`MessageBubble.svelte`) that renders user and assistant chat messages
- **ChatPane**: The Svelte component (`ChatPane.svelte`) that contains the chat interface including input area and status bar
- **Mockup**: The reference HTML file at `mockups/eni-ui-mockup.html` with its associated `mockups/styles.css`

## Requirements

### Requirement 1: Centralized Theme Variables

**User Story:** As a developer, I want all design tokens defined in a single centralized location, so that theme changes only require editing one file.

#### Acceptance Criteria

1. THE Frontend SHALL define all CSS custom properties (--accent, --accent-hover, --accent-muted, --accent-border, --bg-deep, --bg-surface, --bg-elevated, --surface-hover, --border, --text, --text-secondary, --text-muted, --success, --warning, --error, --radius, --radius-sm, --font, --mono) in a single `:root` block within `App.svelte` or a dedicated `theme.css` file.
2. WHEN Theme_Variables are defined centrally, THE Frontend SHALL use the amber/gold palette values from the Mockup (--accent: #e8a33d, --accent-hover: #f0b856, --accent-muted: rgba(232, 163, 61, 0.12), --accent-border: rgba(232, 163, 61, 0.35)).
3. WHEN Theme_Variables are defined centrally, THE Frontend SHALL use the neutral background values from the Mockup (--bg-deep: #1b1b1b, --bg-surface: #262626, --bg-elevated: #2b2b2b, --surface-hover: #333333, --border: #3e3e3e).
4. WHEN Theme_Variables are defined centrally, THE Frontend SHALL use the text color values from the Mockup (--text: #cccccc, --text-secondary: #999999, --text-muted: #666666).
5. WHEN Theme_Variables are centralized, THE Frontend components SHALL remove per-component CSS fallback values for variables that are defined in the central `:root` block.

### Requirement 2: Full-Width Tool Cards and Thinking Blocks

**User Story:** As a user, I want tool call cards and thinking blocks to span the full width of the chat area, so that the interface matches the mockup layout.

#### Acceptance Criteria

1. THE ToolCallCard SHALL use `align-self: stretch` to span the full width of the chat message container.
2. THE ThinkingBlock SHALL use `align-self: stretch` to span the full width of the chat message container.
3. THE ThinkingBlock SHALL remove the `max-width: 90%` constraint.

### Requirement 3: Accent-Colored Element Text Updates

**User Story:** As a user, I want text on accent-colored backgrounds to be dark, so that the contrast is readable against the amber/gold accent color.

#### Acceptance Criteria

1. THE ChatPane send button SHALL display text with color `#1b1b1b` instead of white.
2. THE ChatPane avatar (`.eni-avatar`) SHALL display text with color `#1b1b1b` instead of white.

### Requirement 4: User Message Bubble Styling

**User Story:** As a user, I want my sent messages to have the amber-tinted styling from the mockup, so that the chat visually distinguishes user messages with the new theme.

#### Acceptance Criteria

1. THE MessageBubble SHALL style user messages with background `var(--accent-muted)` (rgba(232, 163, 61, 0.12)).
2. THE MessageBubble SHALL style user messages with border `1px solid var(--accent-border)` (rgba(232, 163, 61, 0.35)).

### Requirement 5: Tool Icon and Tag Pill Color Updates

**User Story:** As a user, I want tool icons and tag pills to use the amber/gold accent color, so that the UI is visually consistent with the new theme.

#### Acceptance Criteria

1. THE ToolCallCard tool icon background SHALL use `rgba(232, 163, 61, 0.15)` instead of `rgba(124, 92, 252, 0.15)`.
2. WHERE tag pills are rendered, THE Frontend SHALL use `rgba(232, 163, 61, 0.1)` as the tag pill background instead of `rgba(124, 92, 252, 0.1)`.
