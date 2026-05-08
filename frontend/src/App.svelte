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
    <button class="wb-toggle-btn" onclick={handleOpen} aria-label="Open World Builder">
      🌍
    </button>
  {/if}
  <PanelShell open={panelOpen} onclose={handleClose} />
</div>

<style>
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
    background: var(--bg-elevated, #252542);
    border: 1px solid var(--border, #3a3a5c);
    border-right: none;
    border-radius: 6px 0 0 6px;
    padding: 8px 10px;
    cursor: pointer;
    font-size: 18px;
    z-index: 49;
    transition: background 120ms;
  }

  .wb-toggle-btn:hover {
    background: var(--surface-hover, #2a2a4a);
  }
</style>
