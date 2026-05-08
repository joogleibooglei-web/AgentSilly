<script lang="ts">
  interface Props {
    name: string;
    description: string;
    success?: boolean | undefined;
    isActive?: boolean;
  }

  let { name, description, success = undefined, isActive = false }: Props = $props();
</script>

<div class="tool-card">
  <div class="tool-icon">🔧</div>
  <span class="tool-name">{name}</span>
  <span class="tool-desc">{description}</span>
  <span class="tool-status" class:success={success === true} class:error={success === false}>
    {#if isActive}
      <span class="spinner"></span>
    {:else if success === true}
      ✓
    {:else if success === false}
      ✗
    {/if}
  </span>
</div>

<style>
  .tool-card {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 11px;
    color: var(--text-secondary);
    align-self: stretch;
    animation: fadeIn 200ms ease;
  }

  .tool-icon {
    width: 18px;
    height: 18px;
    border-radius: 3px;
    background: rgba(232, 163, 61, 0.15);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    flex-shrink: 0;
  }

  .tool-name {
    font-family: var(--mono);
    color: var(--accent);
    flex-shrink: 0;
  }

  .tool-desc {
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tool-status {
    margin-left: auto;
    flex-shrink: 0;
  }

  .tool-status.success {
    color: var(--success);
  }

  .tool-status.error {
    color: var(--error);
  }

  .spinner {
    display: inline-block;
    width: 10px;
    height: 10px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: translateY(0); }
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
