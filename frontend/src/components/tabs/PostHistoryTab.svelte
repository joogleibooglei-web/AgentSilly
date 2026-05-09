<script lang="ts">
  import { onDestroy } from 'svelte';
  import { ui } from '../../lib/stores/ui';

  let content: string | null = $state(null);

  const unsubUi = ui.subscribe(($ui) => {
    const raw = $ui.previewData.posthistory;
    content = typeof raw === 'string' ? raw : null;
  });

  onDestroy(unsubUi);
</script>

{#if content}
  <div class="posthistory-tab">
    <pre class="draft-content">{content}</pre>
  </div>
{:else}
  <div class="tab-placeholder">
    <span class="placeholder-icon">📝</span>
    <span class="placeholder-text">Post-History</span>
    <span class="placeholder-hint">Ask ENI to draft post-history instructions to see them here.</span>
  </div>
{/if}

<style>
  .posthistory-tab {
    height: 100%;
    overflow-y: auto;
  }

  .draft-content {
    font-family: var(--mono);
    font-size: 12px;
    line-height: 1.6;
    color: var(--text-secondary);
    background: var(--bg-elevated);
    padding: 16px;
    border-radius: 6px;
    border: 1px solid var(--border);
    white-space: pre-wrap;
    word-wrap: break-word;
    margin: 0;
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
