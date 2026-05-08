<script lang="ts">
  import { config, setActiveModel, type ModelProfile } from '../lib/stores/config';
  import { getWsClient } from '../lib/ws/client';

  let profiles: ModelProfile[] = $state([]);
  let activeModelId: string | null = $state(null);

  const unsubscribe = config.subscribe(($config) => {
    profiles = $config.modelProfiles;
    activeModelId = $config.activeModelId;
  });

  function handleChange(event: Event) {
    const select = event.target as HTMLSelectElement;
    const profileId = select.value;
    setActiveModel(profileId);
    const wsClient = getWsClient();
    wsClient.sendSwitchModel(profileId);
  }

  import { onDestroy } from 'svelte';
  onDestroy(unsubscribe);
</script>

<select class="model-selector" value={activeModelId ?? ''} onchange={handleChange}>
  {#each profiles as profile (profile.id)}
    <option value={profile.id}>{profile.name}</option>
  {/each}
  {#if profiles.length === 0}
    <option value="" disabled>No models</option>
  {/if}
</select>

<style>
  .model-selector {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-secondary);
    font-size: 10px;
    font-family: var(--mono);
    padding: 3px 8px;
    cursor: pointer;
    outline: none;
  }

  .model-selector:hover {
    border-color: var(--accent);
  }
</style>
