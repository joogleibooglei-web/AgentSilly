<script lang="ts">
  import PanelShell from './components/PanelShell.svelte';

  let panelOpen = $state(true);

  function handleClose() {
    panelOpen = false;
  }

  function handleOpen() {
    panelOpen = true;
  }
</script>

<div class="wb-root">
  {#if !panelOpen}
    <button class="wb-toggle-btn" onclick={handleOpen} aria-label="Open Miss ENI">
      🌍
    </button>
  {/if}
  <PanelShell open={panelOpen} onclose={handleClose} />
</div>

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

  .wb-root {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    color: #e0e0e0;
    height: 100%;
  }

  .wb-toggle-btn {
    position: fixed;
    top: 50%;
    right: 0;
    transform: translateY(-50%);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-right: none;
    border-radius: 6px 0 0 6px;
    padding: 8px 10px;
    cursor: pointer;
    font-size: 18px;
    z-index: 49;
    transition: background 120ms;
  }

  .wb-toggle-btn:hover {
    background: var(--surface-hover);
  }
</style>
