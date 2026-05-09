<script lang="ts">
  import { onDestroy } from 'svelte';
  import { ui, closeRightPane, openRightPane, type PanelMode } from '../lib/stores/ui';
  import ChatPane from './ChatPane.svelte';
  import RightPane from './RightPane.svelte';
  import UndoToast from './UndoToast.svelte';

  interface Props {
    open?: boolean;
    onclose?: () => void;
  }

  let { open = true, onclose }: Props = $props();

  let panelMode: PanelMode = $state('chat-only');
  let panelWidth = $state(420);
  let chatWidth = $state(280);
  let isResizing = $state(false);
  let isResizingSplit = $state(false);

  const MIN_WIDTH = 320;
  const MAX_WIDTH = 1200;
  const CHAT_ONLY_WIDTH = 420;
  const SPLIT_WIDTH = 780;
  const MIN_CHAT_WIDTH = 240;
  const MIN_RIGHT_WIDTH = 280;

  const unsubUi = ui.subscribe(($ui) => {
    const prevMode = panelMode;
    panelMode = $ui.panelMode;

    // Expand/contract panel when right pane opens/closes
    if (prevMode !== panelMode) {
      if (panelMode === 'split') {
        panelWidth = Math.max(panelWidth, SPLIT_WIDTH);
        chatWidth = Math.round(panelWidth * 0.4);
      } else {
        panelWidth = CHAT_ONLY_WIDTH;
      }
    }
  });

  function handleClose() {
    onclose?.();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && panelMode === 'split') {
      closeRightPane();
    }
  }

  // Resize handle logic (left edge — resizes whole panel)
  function startResize(event: MouseEvent) {
    event.preventDefault();
    isResizing = true;
    const startX = event.clientX;
    const startWidth = panelWidth;

    function onMouseMove(e: MouseEvent) {
      const delta = startX - e.clientX;
      const newWidth = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, startWidth + delta));
      panelWidth = newWidth;
      // Keep chat width proportional in split mode
      if (panelMode === 'split') {
        chatWidth = Math.min(chatWidth, newWidth - MIN_RIGHT_WIDTH);
      }
    }

    function onMouseUp() {
      isResizing = false;
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
    }

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
  }

  // Split resize handle logic (between chat and right pane)
  function startSplitResize(event: MouseEvent) {
    event.preventDefault();
    isResizingSplit = true;
    const startX = event.clientX;
    const startChatWidth = chatWidth;

    function onMouseMove(e: MouseEvent) {
      const delta = e.clientX - startX;
      const maxChat = panelWidth - MIN_RIGHT_WIDTH;
      const newChatWidth = Math.min(maxChat, Math.max(MIN_CHAT_WIDTH, startChatWidth + delta));
      chatWidth = newChatWidth;
    }

    function onMouseUp() {
      isResizingSplit = false;
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
      <span class="header-title">Miss ENI</span>
      <div class="header-actions">
        <button class="header-btn settings-btn" onclick={() => openRightPane('settings')} aria-label="Open settings" title="Settings">⚙</button>
        <button class="header-btn close-btn" onclick={handleClose} aria-label="Close panel">✕</button>
      </div>
    </div>

    <!-- Content area -->
    <div class="content">
      {#if panelMode === 'split'}
        <div class="chat-wrapper" style="width: {chatWidth}px;">
          <ChatPane />
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="split-resize-handle"
          class:active={isResizingSplit}
          onmousedown={startSplitResize}
        ></div>
        <RightPane />
      {:else}
        <ChatPane />
      {/if}
    </div>

    <UndoToast />
  </div>
{/if}

<style>
  .panel {
    height: calc(100% - 40px);
    background: var(--bg-deep);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    position: fixed;
    top: 40px;
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
    background: var(--accent);
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .header-title {
    font-weight: 600;
    font-size: 14px;
    letter-spacing: 0.02em;
    color: var(--text);
  }

  .header-actions {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .header-btn {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-muted);
    cursor: pointer;
    border-radius: 4px;
    padding: 5px 8px;
    font-size: 11px;
    font-family: var(--mono);
    transition: all 120ms;
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .header-btn:hover {
    color: var(--text);
    background: var(--surface-hover);
  }

  .content {
    flex: 1;
    display: flex;
    overflow: hidden;
  }

  .chat-wrapper {
    flex-shrink: 0;
    display: flex;
    overflow: hidden;
    border-right: 1px solid var(--border);
  }

  .split-resize-handle {
    width: 4px;
    cursor: col-resize;
    background: transparent;
    transition: background 150ms;
    flex-shrink: 0;
  }

  .split-resize-handle:hover,
  .split-resize-handle.active {
    background: var(--accent);
  }

  @keyframes slideIn {
    from { opacity: 0; transform: translateX(12px); }
    to { opacity: 1; transform: translateX(0); }
  }
</style>
