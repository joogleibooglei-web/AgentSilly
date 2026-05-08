<script lang="ts">
  import { onDestroy } from 'svelte';
  import { ui, closeRightPane, type PanelMode } from '../lib/stores/ui';
  import ChatPane from './ChatPane.svelte';
  import RightPane from './RightPane.svelte';
  import UndoToast from './UndoToast.svelte';

  interface Props {
    open?: boolean;
    onclose?: () => void;
  }

  let { open = true, onclose }: Props = $props();

  let panelMode: PanelMode = $state('chat-only');
  let panelWidth = $state(480);
  let isResizing = $state(false);

  const MIN_WIDTH = 320;
  const MAX_WIDTH = 900;

  const unsubUi = ui.subscribe(($ui) => {
    panelMode = $ui.panelMode;
  });

  function handleClose() {
    onclose?.();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && panelMode === 'split') {
      closeRightPane();
    }
  }

  // Resize handle logic
  function startResize(event: MouseEvent) {
    event.preventDefault();
    isResizing = true;
    const startX = event.clientX;
    const startWidth = panelWidth;

    function onMouseMove(e: MouseEvent) {
      const delta = startX - e.clientX;
      const newWidth = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, startWidth + delta));
      panelWidth = newWidth;
    }

    function onMouseUp() {
      isResizing = false;
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
    }

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
  }

  onDestroy(unsubUi);
</script>

<svelte:window on:keydown={handleKeydown} />

{#if open}
  <div
    class="panel"
    class:split={panelMode === 'split'}
    style="width: {panelWidth}px;"
  >
    <!-- Resize handle -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="resize-handle"
      class:active={isResizing}
      onmousedown={startResize}
    ></div>

    <!-- Header -->
    <div class="header">
      <span class="header-title">World Builder</span>
      <div class="header-actions">
        <button class="header-btn close-btn" onclick={handleClose} aria-label="Close panel">✕</button>
      </div>
    </div>

    <!-- Content area -->
    <div class="content">
      <ChatPane />
      {#if panelMode === 'split'}
        <RightPane />
      {/if}
    </div>

    <UndoToast />
  </div>
{/if}

<style>
  .panel {
    height: 100%;
    background: var(--bg-deep, #1a1a2e);
    border-left: 1px solid var(--border, #3a3a5c);
    display: flex;
    flex-direction: column;
    position: fixed;
    top: 0;
    right: 0;
    z-index: 50;
    animation: slideIn 200ms ease;
  }

  .resize-handle {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 4px;
    cursor: col-resize;
    background: transparent;
    transition: background 150ms;
    z-index: 10;
  }

  .resize-handle:hover,
  .resize-handle.active {
    background: var(--accent, #7c5cfc);
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    background: var(--bg-elevated, #252542);
    border-bottom: 1px solid var(--border, #3a3a5c);
    flex-shrink: 0;
  }

  .header-title {
    font-weight: 600;
    font-size: 14px;
    letter-spacing: 0.02em;
    color: var(--text, #e0e0e0);
  }

  .header-actions {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .header-btn {
    background: transparent;
    border: 1px solid var(--border, #3a3a5c);
    color: var(--text-muted, #6b6b8a);
    cursor: pointer;
    border-radius: 4px;
    padding: 5px 8px;
    font-size: 11px;
    font-family: var(--mono, 'JetBrains Mono', 'Fira Code', monospace);
    transition: all 120ms;
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .header-btn:hover {
    color: var(--text, #e0e0e0);
    background: var(--surface-hover, #2a2a4a);
  }

  .content {
    flex: 1;
    display: flex;
    overflow: hidden;
  }

  /* When in split mode, constrain the chat pane */
  .panel.split :global(.chat-pane) {
    width: 40%;
    min-width: 280px;
    flex: none;
    border-right: 1px solid var(--border, #3a3a5c);
  }

  @keyframes slideIn {
    from { opacity: 0; transform: translateX(12px); }
    to { opacity: 1; transform: translateX(0); }
  }
</style>
