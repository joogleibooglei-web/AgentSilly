<script lang="ts">
  import { onDestroy } from 'svelte';
  import { ui } from '../../lib/stores/ui';

  interface WorldEntry {
    id?: string;
    label: string;
    keywords?: string[];
    content: string;
  }

  let entries: WorldEntry[] = $state([]);
  let copySuccessId: string | null = $state(null);

  const unsubUi = ui.subscribe(($ui) => {
    const raw = $ui.previewData.world;
    if (Array.isArray(raw)) {
      entries = raw as WorldEntry[];
    } else {
      entries = [];
    }
  });

  function formatEntryForClipboard(entry: WorldEntry): string {
    const parts: string[] = [];
    parts.push(`Title: ${entry.label}`);
    if (entry.keywords?.length) parts.push(`Keywords: ${entry.keywords.join(', ')}`);
    parts.push(`\nContent:\n${entry.content}`);
    return parts.join('\n');
  }

  async function copyEntry(entry: WorldEntry) {
    const text = formatEntryForClipboard(entry);
    const entryId = entry.id || entry.label;
    try {
      await navigator.clipboard.writeText(text);
      copySuccessId = entryId;
      setTimeout(() => { copySuccessId = null; }, 2000);
    } catch {
      // Fallback for older browsers
      const textarea = document.createElement('textarea');
      textarea.value = text;
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand('copy');
      document.body.removeChild(textarea);
      copySuccessId = entryId;
      setTimeout(() => { copySuccessId = null; }, 2000);
    }
  }

  onDestroy(unsubUi);
</script>

{#if entries.length > 0}
  <div class="world-tab">
    <div class="entries-list">
      {#each entries as entry (entry.id || entry.label)}
        <div class="entry-card">
          <div class="entry-header">
            <div class="entry-meta">
              <h3 class="entry-title">{entry.label}</h3>
              {#if entry.keywords && entry.keywords.length > 0}
                <div class="tag-list">
                  {#each entry.keywords as keyword}
                    <span class="tag-pill">{keyword}</span>
                  {/each}
                </div>
              {/if}
            </div>
            <button
              class="copy-btn"
              onclick={() => copyEntry(entry)}
              aria-label="Copy entry to clipboard"
            >
              {#if copySuccessId === (entry.id || entry.label)}
                ✓
              {:else}
                📋
              {/if}
            </button>
          </div>
          <div class="entry-content">{entry.content}</div>
        </div>
      {/each}
    </div>
  </div>
{:else}
  <div class="tab-placeholder">
    <span class="placeholder-icon">🌍</span>
    <span class="placeholder-text">World entries</span>
    <span class="placeholder-hint">Ask ENI to show world entries to see them here.</span>
  </div>
{/if}

<style>
  .world-tab {
    height: 100%;
    overflow-y: auto;
  }

  .entries-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .entry-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 14px;
  }

  .entry-header {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin-bottom: 10px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border);
  }

  .entry-meta {
    flex: 1;
    min-width: 0;
  }

  .entry-title {
    font-size: 13px;
    font-weight: 600;
    margin-bottom: 6px;
    color: var(--text);
  }

  .tag-list {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }

  .tag-pill {
    font-size: 9px;
    padding: 2px 6px;
    background: rgba(232, 163, 61, 0.1);
    color: var(--accent);
    border-radius: 3px;
    font-family: var(--mono);
  }

  .copy-btn {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-muted);
    cursor: pointer;
    border-radius: 4px;
    padding: 5px 8px;
    font-size: 12px;
    transition: all 120ms;
    flex-shrink: 0;
  }

  .copy-btn:hover {
    color: var(--text);
    background: var(--surface-hover);
  }

  .entry-content {
    font-size: 12px;
    line-height: 1.6;
    color: var(--text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
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
