<script lang="ts">
  import { ui, setUndoAvailable, type UndoInfo } from '../lib/stores/ui';
  import { getWsClient } from '../lib/ws/client';
  import { onDestroy } from 'svelte';

  let undoInfo: UndoInfo | null = $state(null);
  let dismissTimer: ReturnType<typeof setTimeout> | null = null;

  const unsubscribe = ui.subscribe(($ui) => {
    const newInfo = $ui.undoAvailable;
    if (newInfo && newInfo !== undoInfo) {
      undoInfo = newInfo;
      resetDismissTimer();
    } else if (!newInfo) {
      undoInfo = null;
    }
  });

  function resetDismissTimer() {
    if (dismissTimer) clearTimeout(dismissTimer);
    dismissTimer = setTimeout(() => {
      setUndoAvailable(null);
    }, 10000);
  }

  function handleUndo() {
    if (!undoInfo) return;
    const wsClient = getWsClient();
    wsClient.sendUndo(undoInfo.entityType, undoInfo.entityId);
    setUndoAvailable(null);
  }

  function handleDismiss() {
    setUndoAvailable(null);
  }

  onDestroy(() => {
    unsubscribe();
    if (dismissTimer) clearTimeout(dismissTimer);
  });
</script>

{#if undoInfo}
  <div class="undo-toast">
    <span class="undo-summary">✓ {undoInfo.summary}</span>
    <button class="undo-btn" onclick={handleUndo}>Undo</button>
    <button class="undo-dismiss" onclick={handleDismiss}>✕</button>
  </div>
{/if}

<style>
  .undo-toast {
    position: fixed;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 9px 16px;
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.4);
    animation: fadeUp 200ms ease;
    z-index: 100;
    color: var(--text);
  }

  .undo-summary {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 300px;
  }

  .undo-btn {
    background: transparent;
    border: 1px solid var(--accent);
    color: var(--accent);
    border-radius: 4px;
    padding: 3px 10px;
    font-size: 11px;
    cursor: pointer;
    font-family: inherit;
    transition: all 120ms;
    white-space: nowrap;
  }

  .undo-btn:hover {
    background: var(--accent);
    color: white;
  }

  .undo-dismiss {
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 12px;
    padding: 2px;
    line-height: 1;
  }

  .undo-dismiss:hover {
    color: var(--text);
  }

  @keyframes fadeUp {
    from { opacity: 0; transform: translateX(-50%) translateY(8px); }
    to { opacity: 1; transform: translateX(-50%) translateY(0); }
  }
</style>
