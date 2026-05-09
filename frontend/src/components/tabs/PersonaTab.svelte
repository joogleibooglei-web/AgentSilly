<script lang="ts">
  import { onDestroy } from 'svelte';
  import { ui } from '../../lib/stores/ui';

  interface PersonaData {
    name?: string;
    description?: string;
    title?: string;
    avatar?: string;
    tags?: string[];
    relationship?: string;
  }

  let persona: PersonaData | null = $state(null);
  let copySuccess = $state(false);

  const unsubUi = ui.subscribe(($ui) => {
    persona = $ui.previewData.persona as PersonaData | null;
  });

  function getInitial(name: string | undefined): string {
    if (!name) return '?';
    return name.charAt(0).toUpperCase();
  }

  function formatForClipboard(): string {
    if (!persona) return '';
    const parts: string[] = [];
    if (persona.name) parts.push(`Name: ${persona.name}`);
    if (persona.title) parts.push(`Title: ${persona.title}`);
    if (persona.tags?.length) parts.push(`Tags: ${persona.tags.join(', ')}`);
    if (persona.description) parts.push(`\nDescription:\n${persona.description}`);
    if (persona.relationship) parts.push(`\nRelationship:\n${persona.relationship}`);
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

{#if persona}
  <div class="persona-tab">
    <div class="preview-card">
      <div class="preview-card-header">
        <div class="preview-avatar">{getInitial(persona.name)}</div>
        <div class="preview-meta">
          <h2 class="persona-name">{persona.name || 'Unnamed Persona'}</h2>
          {#if persona.title}
            <span class="persona-title">{persona.title}</span>
          {/if}
          {#if persona.tags && persona.tags.length > 0}
            <div class="tag-list">
              {#each persona.tags as tag}
                <span class="tag-pill">{tag}</span>
              {/each}
            </div>
          {/if}
        </div>
        <button class="copy-btn" onclick={copyToClipboard} aria-label="Copy persona data to clipboard">
          {#if copySuccess}
            ✓
          {:else}
            📋
          {/if}
        </button>
      </div>

      <!-- Description -->
      {#if persona.description}
        <div class="section">
          <div class="section-label">Description</div>
          <div class="section-content">{persona.description}</div>
        </div>
      {/if}

      <!-- Relationship to active character -->
      {#if persona.relationship}
        <div class="section">
          <div class="section-label">Relationship</div>
          <div class="section-content">{persona.relationship}</div>
        </div>
      {/if}
    </div>
  </div>
{:else}
  <div class="tab-placeholder">
    <span class="placeholder-icon">🎭</span>
    <span class="placeholder-text">Persona preview</span>
    <span class="placeholder-hint">Ask ENI to read or create a persona to see a preview here.</span>
  </div>
{/if}

<style>
  .persona-tab {
    height: 100%;
  }

  .preview-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 16px;
  }

  .preview-card-header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 14px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border);
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
    background: linear-gradient(135deg, var(--accent), #ff6b9d);
    flex-shrink: 0;
  }

  .preview-meta {
    flex: 1;
    min-width: 0;
  }

  .persona-name {
    font-size: 15px;
    font-weight: 600;
    margin-bottom: 2px;
    color: var(--text);
  }

  .persona-title {
    font-size: 11px;
    color: var(--text-muted);
    font-style: italic;
    margin-bottom: 4px;
    display: block;
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

  .section {
    margin-bottom: 12px;
  }

  .section:last-child {
    margin-bottom: 0;
  }

  .section-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 4px;
  }

  .section-content {
    font-size: 12px;
    line-height: 1.6;
    color: var(--text-secondary);
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
