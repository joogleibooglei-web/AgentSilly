<script lang="ts">
  import { onDestroy } from 'svelte';
  import {
    ui,
    activeTab,
    closeRightPane,
    switchTab,
    type RightPaneTab,
  } from '../lib/stores/ui';
  import CharacterTab from './tabs/CharacterTab.svelte';
  import WorldTab from './tabs/WorldTab.svelte';
  import PostHistoryTab from './tabs/PostHistoryTab.svelte';
  import PersonaTab from './tabs/PersonaTab.svelte';
  import SettingsTab from './tabs/SettingsTab.svelte';

  const tabs: { id: RightPaneTab; label: string }[] = [
    { id: 'character', label: 'Character' },
    { id: 'world', label: 'World' },
    { id: 'posthistory', label: 'Post-History' },
    { id: 'persona', label: 'Persona' },
    { id: 'settings', label: 'Settings' },
  ];

  let currentTab: RightPaneTab = $state('character');
  let previewData: Record<RightPaneTab, unknown> = $state({
    character: null,
    world: null,
    posthistory: null,
    persona: null,
    settings: null,
  });

  const unsubUi = ui.subscribe(($ui) => {
    currentTab = $ui.activeTab;
    previewData = $ui.previewData;
  });

  function handleTabClick(tab: RightPaneTab) {
    switchTab(tab);
  }

  function handleClose() {
    closeRightPane();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      closeRightPane();
    }
  }

  onDestroy(unsubUi);
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="right-pane">
  <!-- Tab bar -->
  <div class="tab-bar">
    <div class="tabs">
      {#each tabs as tab (tab.id)}
        <button
          class="tab"
          class:active={currentTab === tab.id}
          onclick={() => handleTabClick(tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </div>
    <button class="close-btn" onclick={handleClose} aria-label="Close right pane">✕</button>
  </div>

  <!-- Tab content -->
  <div class="tab-content">
    {#if currentTab === 'character'}
      <CharacterTab />
    {:else if currentTab === 'world'}
      <WorldTab />
    {:else if currentTab === 'posthistory'}
      <PostHistoryTab />
    {:else if currentTab === 'persona'}
      <PersonaTab />
    {:else if currentTab === 'settings'}
      <SettingsTab />
    {/if}
  </div>
</div>

<style>
  .right-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-left: 1px solid var(--border);
    background: var(--bg-deep);
    min-width: 0;
  }

  .tab-bar {
    display: flex;
    align-items: center;
    gap: 0;
    padding: 0 8px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .tabs {
    display: flex;
    flex: 1;
    overflow-x: auto;
    gap: 0;
  }

  .tab {
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-muted);
    cursor: pointer;
    padding: 10px 12px;
    font-size: 11px;
    font-weight: 500;
    font-family: inherit;
    white-space: nowrap;
    transition: color 120ms, border-color 120ms;
  }

  .tab:hover {
    color: var(--text);
  }

  .tab.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }

  .close-btn {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-muted);
    cursor: pointer;
    border-radius: 4px;
    padding: 4px 7px;
    font-size: 11px;
    font-family: var(--mono);
    transition: all 120ms;
    flex-shrink: 0;
    margin-left: 8px;
  }

  .close-btn:hover {
    color: var(--text);
    background: var(--surface-hover);
  }

  .tab-content {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
  }
</style>
