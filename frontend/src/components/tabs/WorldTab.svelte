<script lang="ts">
  import { onDestroy } from 'svelte';
  import { ui } from '../../lib/stores/ui';

  let content: string | null = $state(null);

  const unsubUi = ui.subscribe(($ui) => {
    const raw = $ui.previewData.world;
    content = typeof raw === 'string' ? raw : null;
  });

  onDestroy(unsubUi);
</script>

{#if content}
  <div class="world-tab">
    <pre class="draft-content">{content}</pre>
  </div>
{:else}
  <div class="tab-placeholder">
    <span class="placeholder-icon">🌍</span>
    <span class="placeholder-text">World Info</span>
    <span class="placeholder-hint">Ask ENI to draft world info to see it here.</span>
  </div>
{/if}

<style>
  .world-tab {
    height: 100%;
    overflow-y: auto;
  }

  .draft-content {
    font-size: 12px;
    line-height: 1.6;
    color: var(--text-secondary);
    white-space: pre-wrap;
    word-wrap: break-word;
    margin: 0;
    padding: 14px;
    font-family: var(--mono);
  }

  .tab-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 8px;
    color: var(--text-muted);
  }

  .placeholder-icon {
    font-size: 32px;
    opacity: 0.6;
  }

  .placeholder-text {
    font-size: 13px;
    font-weight: 500;
    color: var(--text);
  }

  .placeholder-hint {
    font-size: 11px;
    color: var(--text-muted);
    text-align: center;
    max-width: 200px;
  }
</style>
