<script lang="ts">
  import { onDestroy } from 'svelte';
  import { ui } from '../../lib/stores/ui';
  import { config } from '../../lib/stores/config';

  interface CharacterData {
    name?: string;
    avatar?: string;
    description?: string;
    personality?: string;
    scenario?: string;
    first_mes?: string;
    first_message?: string;
    mes_example?: string;
    creator_notes?: string;
    system_prompt?: string;
    post_history_instructions?: string;
    tags?: string[];
    alternate_greetings?: string[];
    creator?: string;
    character_version?: string;
    talkativeness?: number;
    character_book?: unknown;
    extensions?: unknown;
  }

  let character: CharacterData | null = $state(null);
  let stBaseUrl = $state('http://localhost:8000');
  let copySuccess = $state(false);
  let avatarError = $state(false);

  const unsubUi = ui.subscribe(($ui) => {
    character = $ui.previewData.character as CharacterData | null;
    avatarError = false;
  });

  const unsubConfig = config.subscribe(($config) => {
    stBaseUrl = $config.stBaseUrl;
  });

  function getInitial(name: string | undefined): string {
    if (!name) return '?';
    return name.charAt(0).toUpperCase();
  }

  function getAvatarUrl(avatar: string | undefined): string | null {
    if (!avatar) return null;
    // SillyTavern serves avatars at /characters/<filename>
    return `${stBaseUrl}/characters/${encodeURIComponent(avatar)}`;
  }

  function getFirstMessage(): string | undefined {
    return character?.first_mes || character?.first_message;
  }

  function hasContent(value: unknown): boolean {
    if (value === null || value === undefined) return false;
    if (typeof value === 'string') return value.trim().length > 0;
    if (Array.isArray(value)) return value.length > 0;
    if (typeof value === 'number') return true;
    if (typeof value === 'object') return Object.keys(value as object).length > 0;
    return false;
  }

  function formatForClipboard(): string {
    if (!character) return '';
    const parts: string[] = [];
    if (character.name) parts.push(`Name: ${character.name}`);
    if (character.tags?.length) parts.push(`Tags: ${character.tags.join(', ')}`);
    if (hasContent(character.description)) parts.push(`\nDescription:\n${character.description}`);
    if (hasContent(character.personality)) parts.push(`\nPersonality:\n${character.personality}`);
    if (hasContent(character.scenario)) parts.push(`\nScenario:\n${character.scenario}`);
    if (hasContent(getFirstMessage())) parts.push(`\nFirst Message:\n${getFirstMessage()}`);
    if (hasContent(character.mes_example)) parts.push(`\nExample Dialogue:\n${character.mes_example}`);
    if (hasContent(character.system_prompt)) parts.push(`\nSystem Prompt:\n${character.system_prompt}`);
    if (hasContent(character.post_history_instructions)) parts.push(`\nPost-History Instructions:\n${character.post_history_instructions}`);
    if (hasContent(character.creator_notes)) parts.push(`\nCreator Notes:\n${character.creator_notes}`);
    if (character.alternate_greetings?.length) parts.push(`\nAlternate Greetings:\n${character.alternate_greetings.join('\n---\n')}`);
    if (hasContent(character.creator)) parts.push(`\nCreator: ${character.creator}`);
    if (hasContent(character.character_version)) parts.push(`Version: ${character.character_version}`);
    if (character.talkativeness != null) parts.push(`Talkativeness: ${character.talkativeness}`);
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

  onDestroy(() => {
    unsubUi();
    unsubConfig();
  });
</script>

{#if character}
  <div class="character-tab">
    <div class="preview-card">
      <!-- Header: Avatar + Name + Tags -->
      <div class="preview-card-header">
        {#if !avatarError && getAvatarUrl(character.avatar)}
          <img
            class="preview-avatar-img"
            src={getAvatarUrl(character.avatar)}
            alt={character.name || 'Character'}
            onerror={() => { avatarError = true; }}
          />
        {:else}
          <div class="preview-avatar">{getInitial(character.name)}</div>
        {/if}
        <div class="preview-meta">
          <h2 class="character-name">{character.name || 'Unnamed Character'}</h2>
          {#if character.creator}
            <span class="creator-label">by {character.creator}</span>
          {/if}
        </div>
        <button class="copy-btn" onclick={copyToClipboard} aria-label="Copy character data to clipboard">
          {#if copySuccess}
            ✓
          {:else}
            📋
          {/if}
        </button>
      </div>

      <!-- Tags -->
      {#if character.tags && character.tags.length > 0}
        <div class="tag-list">
          {#each character.tags as tag}
            <span class="tag-pill">{tag}</span>
          {/each}
        </div>
      {/if}

      <!-- Description -->
      {#if hasContent(character.description)}
        <div class="section">
          <div class="section-label">Description</div>
          <div class="section-content">{character.description}</div>
        </div>
      {/if}

      <!-- Personality -->
      {#if hasContent(character.personality)}
        <div class="section">
          <div class="section-label">Personality</div>
          <div class="section-content">{character.personality}</div>
        </div>
      {/if}

      <!-- Scenario -->
      {#if hasContent(character.scenario)}
        <div class="section">
          <div class="section-label">Scenario</div>
          <div class="section-content">{character.scenario}</div>
        </div>
      {/if}

      <!-- First Message -->
      {#if hasContent(getFirstMessage())}
        <div class="section">
          <div class="section-label">First Message</div>
          <div class="section-content mono">{getFirstMessage()}</div>
        </div>
      {/if}

      <!-- Example Dialogue -->
      {#if hasContent(character.mes_example)}
        <div class="section">
          <div class="section-label">Example Dialogue</div>
          <div class="section-content mono">{character.mes_example}</div>
        </div>
      {/if}

      <!-- System Prompt -->
      {#if hasContent(character.system_prompt)}
        <div class="section">
          <div class="section-label">System Prompt</div>
          <div class="section-content mono">{character.system_prompt}</div>
        </div>
      {/if}

      <!-- Post-History Instructions -->
      {#if hasContent(character.post_history_instructions)}
        <div class="section">
          <div class="section-label">Post-History Instructions</div>
          <div class="section-content mono">{character.post_history_instructions}</div>
        </div>
      {/if}

      <!-- Creator Notes -->
      {#if hasContent(character.creator_notes)}
        <div class="section">
          <div class="section-label">Creator Notes</div>
          <div class="section-content">{character.creator_notes}</div>
        </div>
      {/if}

      <!-- Alternate Greetings -->
      {#if character.alternate_greetings && character.alternate_greetings.length > 0}
        <div class="section">
          <div class="section-label">Alternate Greetings ({character.alternate_greetings.length})</div>
          {#each character.alternate_greetings as greeting, i}
            <div class="section-content mono alt-greeting">
              <span class="alt-greeting-num">#{i + 1}</span>
              {greeting}
            </div>
          {/each}
        </div>
      {/if}

      <!-- Metadata footer -->
      {#if hasContent(character.character_version) || character.talkativeness != null}
        <div class="section metadata-footer">
          {#if hasContent(character.character_version)}
            <span class="meta-item">v{character.character_version}</span>
          {/if}
          {#if character.talkativeness != null}
            <span class="meta-item">Talkativeness: {character.talkativeness}</span>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{:else}
  <div class="tab-placeholder">
    <span class="placeholder-icon">👤</span>
    <span class="placeholder-text">Character preview</span>
    <span class="placeholder-hint">Ask ENI to read or create a character to see a preview here.</span>
  </div>
{/if}

<style>
  .character-tab {
    height: 100%;
    overflow-y: auto;
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

  .preview-avatar-img {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    object-fit: cover;
    flex-shrink: 0;
    border: 2px solid var(--border);
  }

  .preview-avatar {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 22px;
    font-weight: 700;
    color: white;
    background: linear-gradient(135deg, var(--accent), #ff6b9d);
    flex-shrink: 0;
  }

  .preview-meta {
    flex: 1;
    min-width: 0;
  }

  .character-name {
    font-size: 16px;
    font-weight: 600;
    margin-bottom: 2px;
    color: var(--text);
  }

  .creator-label {
    font-size: 11px;
    color: var(--text-muted);
    font-style: italic;
  }

  .tag-list {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    margin-bottom: 14px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border);
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
    margin-bottom: 14px;
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
    white-space: pre-wrap;
    word-break: break-word;
  }

  .section-content.mono {
    font-family: var(--mono);
    font-size: 11px;
    background: var(--bg-elevated);
    padding: 10px;
    border-radius: 4px;
  }

  .alt-greeting {
    margin-bottom: 8px;
    position: relative;
  }

  .alt-greeting:last-child {
    margin-bottom: 0;
  }

  .alt-greeting-num {
    font-size: 9px;
    font-weight: 700;
    color: var(--accent);
    margin-right: 6px;
  }

  .metadata-footer {
    display: flex;
    gap: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }

  .meta-item {
    font-size: 10px;
    color: var(--text-muted);
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
