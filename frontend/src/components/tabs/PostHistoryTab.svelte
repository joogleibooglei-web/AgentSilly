<script lang="ts">
  import { onDestroy } from 'svelte';
  import { ui } from '../../lib/stores/ui';

  interface PostHistoryData {
    narration_style?: string;
    formatting_rules?: string;
    tone_keywords?: string[];
  }

  let postHistory: PostHistoryData | null = $state(null);
  let copySuccess = $state(false);

  const unsubUi = ui.subscribe(($ui) => {
    postHistory = $ui.previewData.posthistory as PostHistoryData | null;
  });

  function formatForClipboard(): string {
    if (!postHistory) return '';
    const parts: string[] = [];
    if (postHistory.narration_style) parts.push(`Narration Style:\n${postHistory.narration_style}`);
    if (postHistory.formatting_rules) parts.push(`\nFormatting Rules:\n${postHistory.formatting_rules}`);
    if (postHistory.tone_keywords?.length) parts.push(`\nTone Keywords: ${postHistory.tone_keywords.join(', ')}`);
    return parts.join('\n');
  }

  async function copyToClipboard() {
    const text = formatForClipboard();
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      copySuccess = true;
      setTimeout(() => { copySuccess = false; }, 2000);
    } catch {
      // Fallback for older browsers
      const textarea = document.createElement('textarea');
      textarea.value = text;
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand('copy');
      document.body.removeChild(textarea);
      copySuccess = true;
      setTimeout(() => { copySuccess = false; }, 2000);
    }
  }

  onDestroy(unsubUi);
</script>

{#if postHistory}
  <div class="posthistory-tab">
    <div class="preview-card">
      <div class="preview-card-header">
        <div class="preview-avatar">📝</div>
        <div class="preview-meta">
          <h2 class="card-title">Post-History</h2>
          {#if postHistory.tone_keywords && postHistory.tone_keywords.length > 0}
            <div class="tag-list">
              {#each postHistory.tone_keywords as keyword}
                <span class="tag-pill">{keyword}</span>
              {/each}
            </div>
          {/if}
        </div>
        <button class="copy-btn" onclick={copyToClipboard} aria-label="Copy post-history data to clipboard">
          {#if copySuccess}
            ✓
          {:else}
            📋
          {/if}
        </button>
      </div>

      <!-- Narration Style -->
      {#if postHistory.narration_style}
        <div class="section">
          <div class="section-label">Narration Style</div>
          <div class="section-content mono">{postHistory.narration_style}</div>
        </div>
      {/if}

      <!-- Formatting Rules -->
      {#if postHistory.formatting_rules}
        <div class="section">
          <div class="section-label">Formatting Rules</div>
          <div class="section-content mono">{postHistory.formatting_rules}</div>
        </div>
      {/if}
    </div>
  </div>
{:else}
  <div class="tab-placeholder">
    <span class="placeholder-icon">📝</span>
    <span class="placeholder-text">Post-History</span>
    <span class="placeholder-hint">Ask ENI to generate post-history settings to see them here.</span>
  </div>
{/if}

<style>
  .posthistory-tab {
    height: 100%;
  }

  .preview-card {
    background: var(--bg-surface, #1f1f36);
    border: 1px solid var(--border, #3a3a5c);
    border-radius: 6px;
    padding: 16px;
  }

  .preview-card-header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 14px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border, #3a3a5c);
  }

  .preview-avatar {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 20px;
    font-weight: 700;
    color: white;
    background: linear-gradient(135deg, var(--accent, #7c5cfc), #ff6b9d);
    flex-shrink: 0;
  }

  .preview-meta {
    flex: 1;
    min-width: 0;
  }

  .card-title {
    font-size: 15px;
    font-weight: 600;
    margin-bottom: 4px;
    color: var(--text, #e0e0e0);
  }

  .tag-list {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }

  .tag-pill {
    font-size: 9px;
    padding: 2px 6px;
    background: rgba(124, 92, 252, 0.1);
    color: var(--accent, #7c5cfc);
    border-radius: 3px;
    font-family: var(--mono, 'JetBrains Mono', 'Fira Code', monospace);
  }

  .copy-btn {
    background: transparent;
    border: 1px solid var(--border, #3a3a5c);
    color: var(--text-muted, #6b6b8a);
    cursor: pointer;
    border-radius: 4px;
    padding: 5px 8px;
    font-size: 12px;
    transition: all 120ms;
    flex-shrink: 0;
  }

  .copy-btn:hover {
    color: var(--text, #e0e0e0);
    background: var(--surface-hover, #2a2a4a);
  }

  .section {
    margin-bottom: 12px;
  }

  .section:last-child {
    margin-bottom: 0;
  }

  .section-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted, #6b6b8a);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 4px;
  }

  .section-content {
    font-size: 12px;
    line-height: 1.6;
    color: var(--text-secondary, #a0a0a0);
  }

  .section-content.mono {
    font-family: var(--mono, 'JetBrains Mono', 'Fira Code', monospace);
    font-size: 11px;
    background: var(--bg-elevated, #252542);
    padding: 10px;
    border-radius: 4px;
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
    color: var(--text-muted, #6b6b8a);
  }

  .placeholder-icon {
    font-size: 32px;
    opacity: 0.6;
  }

  .placeholder-text {
    font-size: 13px;
    font-weight: 500;
    color: var(--text, #e0e0e0);
  }

  .placeholder-hint {
    font-size: 11px;
    color: var(--text-muted, #6b6b8a);
    text-align: center;
    max-width: 200px;
  }
</style>
