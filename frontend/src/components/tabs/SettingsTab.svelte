<script lang="ts">
  import { onDestroy } from 'svelte';
  import { config, type ModelProfile, type ConfigStore } from '../../lib/stores/config';
  import { connection, type ConnectionStore, type ConnectionState } from '../../lib/stores/connection';
  import { getWsClient } from '../../lib/ws/client';

  // Local state bound to form inputs
  let baseUrl = $state('');
  let apiKey = $state('');
  let model = $state('');
  let temperature = $state(0.7);
  let maxTokens = $state(4096);

  let postCardPrompt = $state('');
  let connectionState: ConnectionState = $state('disconnected');

  // Available models fetched from the API
  let availableModels: string[] = $state([]);
  let fetchingModels = $state(false);

  interface ReferenceDoc {
    name: string;
    size: number;
  }

  let referenceDocuments: ReferenceDoc[] = $state([]);

  // Subscribe to config store
  const unsubConfig = config.subscribe(($config: ConfigStore) => {
    const activeProfile = $config.modelProfiles.find((p) => p.id === $config.activeModelId)
      ?? $config.modelProfiles[0]
      ?? null;

    if (activeProfile) {
      baseUrl = activeProfile.baseUrl;
      apiKey = activeProfile.apiKey;
      model = activeProfile.model;
      temperature = activeProfile.temperature;
      maxTokens = activeProfile.maxTokens;
      // Fetch available models when profile loads with credentials
      if (activeProfile.baseUrl && activeProfile.apiKey) {
        fetchAvailableModels();
      }
    }

    postCardPrompt = $config.postCardPrompt;
  });

  // Subscribe to connection store
  const unsubConnection = connection.subscribe(($conn: ConnectionStore) => {
    connectionState = $conn.state;
  });

  function sendConfigUpdate(key: string, value: unknown): void {
    const ws = getWsClient();
    ws.send({ type: 'update_config', key, value });
  }

  function handleProfileFieldBlur(field: string, value: unknown): void {
    sendConfigUpdate(`model_profile.${field}`, value);
  }

  function handleTestConnection(): void {
    const ws = getWsClient();
    ws.send({ type: 'test_connection' } as any);
  }

  function handleConnect(): void {
    const ws = getWsClient();
    ws.connect();
  }

  function handleDisconnect(): void {
    const ws = getWsClient();
    ws.disconnect();
  }

  /** Fetch available models from the configured base URL */
  async function fetchAvailableModels(): Promise<void> {
    if (!baseUrl || !apiKey) return;
    fetchingModels = true;
    try {
      const url = `${baseUrl.replace(/\/$/, '')}/models`;
      const resp = await fetch(url, {
        headers: { 'Authorization': `Bearer ${apiKey}` },
      });
      if (resp.ok) {
        const data = await resp.json();
        const models: string[] = (data.data || data)
          .map((m: { id?: string }) => m.id)
          .filter((id: unknown): id is string => typeof id === 'string')
          .sort();
        availableModels = models;
      } else {
        availableModels = [];
      }
    } catch {
      availableModels = [];
    }
    fetchingModels = false;
  }

  function handleModelChange(event: Event): void {
    const select = event.target as HTMLSelectElement;
    model = select.value;
    handleProfileFieldBlur('model', model);
  }

  function handleBaseUrlBlur(): void {
    handleProfileFieldBlur('baseUrl', baseUrl);
    fetchAvailableModels();
  }

  function handleApiKeyBlur(): void {
    handleProfileFieldBlur('apiKey', apiKey);
    fetchAvailableModels();
  }

  function handlePostCardBlur(): void {
    sendConfigUpdate('postCardPrompt', postCardPrompt);
  }

  function handleFileUpload(event: Event): void {
    const input = event.target as HTMLInputElement;
    if (!input.files || input.files.length === 0) return;

    const file = input.files[0];
    const allowedTypes = ['.txt', '.md'];
    const ext = file.name.substring(file.name.lastIndexOf('.'));

    if (!allowedTypes.includes(ext)) {
      return;
    }

    // Read and send to sidecar
    const reader = new FileReader();
    reader.onload = () => {
      const content = reader.result as string;
      sendConfigUpdate('reference_document_add', { name: file.name, size: file.size, content });
      referenceDocuments = [...referenceDocuments, { name: file.name, size: file.size }];
    };
    reader.readAsText(file);

    // Reset input
    input.value = '';
  }

  function removeDocument(index: number): void {
    const doc = referenceDocuments[index];
    sendConfigUpdate('reference_document_remove', { name: doc.name });
    referenceDocuments = referenceDocuments.filter((_, i) => i !== index);
  }

  function formatFileSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function getStatusColor(state: ConnectionState): string {
    switch (state) {
      case 'connected': return 'var(--status-green, #4caf50)';
      case 'disconnected': return 'var(--status-red, #f44336)';
      case 'reconnecting': return 'var(--status-yellow, #ff9800)';
    }
  }

  function getStatusLabel(state: ConnectionState): string {
    switch (state) {
      case 'connected': return 'Connected';
      case 'disconnected': return 'Disconnected';
      case 'reconnecting': return 'Reconnecting...';
    }
  }

  onDestroy(() => {
    unsubConfig();
    unsubConnection();
  });
</script>

  <div class="settings-tab">
    <!-- Sidecar Connection Status -->
    <div class="section">
      <div class="section-label">Sidecar Connection</div>
      <div class="status-row">
        <span class="status-dot" style="background: {getStatusColor(connectionState)}"></span>
        <span class="status-text">{getStatusLabel(connectionState)}</span>
        {#if connectionState === 'disconnected' || connectionState === 'setup_required'}
          <button class="connect-btn" onclick={handleConnect}>Connect</button>
        {:else if connectionState === 'connected'}
          <button class="disconnect-btn" onclick={handleDisconnect}>Disconnect</button>
        {/if}
      </div>
    </div>

    <!-- Model Profile Configuration -->
    <div class="section">
      <div class="section-label">Model Endpoint</div>
      <div class="form-group">
        <label class="form-label" for="settings-base-url">Base URL</label>
        <input
          id="settings-base-url"
          class="form-input"
          type="text"
          bind:value={baseUrl}
          onblur={handleBaseUrlBlur}
          placeholder="https://api.openai.com/v1"
        />
      </div>
      <div class="form-group">
        <label class="form-label" for="settings-api-key">API Key</label>
        <input
          id="settings-api-key"
          class="form-input"
          type="password"
          bind:value={apiKey}
          onblur={handleApiKeyBlur}
          placeholder="sk-..."
        />
      </div>
      <div class="form-group">
        <label class="form-label" for="settings-model">Model</label>
        {#if availableModels.length > 0}
          <select
            id="settings-model"
            class="form-input"
            value={model}
            onchange={handleModelChange}
          >
            {#if model && !availableModels.includes(model)}
              <option value={model}>{model}</option>
            {/if}
            {#each availableModels as m (m)}
              <option value={m}>{m}</option>
            {/each}
          </select>
        {:else}
          <input
            id="settings-model"
            class="form-input"
            type="text"
            bind:value={model}
            onblur={() => handleProfileFieldBlur('model', model)}
            placeholder={fetchingModels ? 'Fetching models...' : 'gpt-4o'}
          />
        {/if}
      </div>
      <div class="form-row">
        <div class="form-group half">
          <label class="form-label" for="settings-temperature">Temperature</label>
          <input
            id="settings-temperature"
            class="form-input"
            type="number"
            min="0"
            max="2"
            step="0.1"
            bind:value={temperature}
            onblur={() => handleProfileFieldBlur('temperature', temperature)}
          />
        </div>
        <div class="form-group half">
          <label class="form-label" for="settings-max-tokens">Max Tokens</label>
          <input
            id="settings-max-tokens"
            class="form-input"
            type="number"
            min="1"
            bind:value={maxTokens}
            onblur={() => handleProfileFieldBlur('maxTokens', maxTokens)}
          />
        </div>
      </div>
      <button class="test-btn" onclick={handleTestConnection}>Test Connection</button>
    </div>

    <!-- Post-Card Prompt -->
    <div class="section">
      <div class="section-label">Post-Card Prompt</div>
      <textarea
        class="form-textarea"
        bind:value={postCardPrompt}
        onblur={handlePostCardBlur}
        placeholder="Optional system prompt appended after ENI's personality card..."
        rows="5"
      ></textarea>
    </div>

    <!-- Reference Documents -->
    <div class="section">
      <div class="section-label">Reference Documents</div>
      <div class="doc-list">
        {#if referenceDocuments.length === 0}
          <div class="doc-empty">No documents uploaded</div>
        {:else}
          {#each referenceDocuments as doc, index (doc.name + index)}
            <div class="doc-item">
              <div class="doc-info">
                <span class="doc-name">{doc.name}</span>
                <span class="doc-size">{formatFileSize(doc.size)}</span>
              </div>
              <button
                class="doc-remove-btn"
                onclick={() => removeDocument(index)}
                aria-label="Remove {doc.name}"
              >✕</button>
            </div>
          {/each}
        {/if}
      </div>
      <label class="upload-btn">
        <input
          type="file"
          accept=".txt,.md"
          onchange={handleFileUpload}
          class="file-input-hidden"
        />
        Upload Document
      </label>
    </div>
  </div>

<style>
  .settings-tab {
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 14px;
  }

  .section-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 10px;
  }

  .status-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .status-text {
    font-size: 12px;
    color: var(--text-secondary);
  }

  .connect-btn {
    margin-left: auto;
    padding: 5px 12px;
    font-size: 11px;
    font-weight: 500;
    color: var(--accent);
    background: rgba(232, 163, 61, 0.1);
    border: 1px solid var(--accent);
    border-radius: 4px;
    cursor: pointer;
    transition: all 120ms;
    font-family: inherit;
  }

  .connect-btn:hover {
    background: rgba(232, 163, 61, 0.2);
  }

  .disconnect-btn {
    margin-left: auto;
    padding: 5px 12px;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-muted);
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 4px;
    cursor: pointer;
    transition: all 120ms;
    font-family: inherit;
  }

  .disconnect-btn:hover {
    color: var(--error);
    border-color: var(--error);
    background: rgba(244, 67, 54, 0.08);
  }

  .form-group {
    margin-bottom: 10px;
  }

  .form-group:last-child {
    margin-bottom: 0;
  }

  .form-row {
    display: flex;
    gap: 10px;
  }

  .form-group.half {
    flex: 1;
  }

  .form-label {
    display: block;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-secondary);
    margin-bottom: 4px;
  }

  .form-input {
    width: 100%;
    padding: 7px 10px;
    font-size: 12px;
    font-family: var(--mono);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text);
    outline: none;
    transition: border-color 120ms;
    box-sizing: border-box;
  }

  .form-input:focus {
    border-color: var(--accent);
  }

  .form-input::placeholder {
    color: var(--text-muted);
  }

  .form-textarea {
    width: 100%;
    padding: 8px 10px;
    font-size: 12px;
    font-family: var(--mono);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text);
    outline: none;
    resize: vertical;
    min-height: 80px;
    transition: border-color 120ms;
    box-sizing: border-box;
  }

  .form-textarea:focus {
    border-color: var(--accent);
  }

  .form-textarea::placeholder {
    color: var(--text-muted);
  }

  .doc-list {
    margin-bottom: 10px;
  }

  .doc-empty {
    font-size: 11px;
    color: var(--text-muted);
    font-style: italic;
    padding: 6px 0;
  }

  .doc-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px;
    background: var(--bg-elevated);
    border-radius: 4px;
    margin-bottom: 4px;
  }

  .doc-item:last-child {
    margin-bottom: 0;
  }

  .doc-info {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .doc-name {
    font-size: 11px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .doc-size {
    font-size: 10px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .doc-remove-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 11px;
    padding: 2px 5px;
    border-radius: 3px;
    transition: all 120ms;
    flex-shrink: 0;
  }

  .doc-remove-btn:hover {
    color: var(--status-red, #f44336);
    background: rgba(244, 67, 54, 0.1);
  }

  .upload-btn {
    display: inline-flex;
    align-items: center;
    padding: 6px 12px;
    font-size: 11px;
    font-weight: 500;
    color: var(--accent);
    background: rgba(232, 163, 61, 0.1);
    border: 1px solid var(--accent);
    border-radius: 4px;
    cursor: pointer;
    transition: all 120ms;
  }

  .upload-btn:hover {
    background: rgba(232, 163, 61, 0.2);
  }

  .file-input-hidden {
    display: none;
  }

  .test-btn {
    display: inline-flex;
    align-items: center;
    margin-top: 10px;
    padding: 7px 14px;
    font-size: 11px;
    font-weight: 500;
    color: var(--accent);
    background: rgba(232, 163, 61, 0.1);
    border: 1px solid var(--accent);
    border-radius: 4px;
    cursor: pointer;
    transition: all 120ms;
  }

  .test-btn:hover {
    background: rgba(232, 163, 61, 0.2);
  }


</style>
