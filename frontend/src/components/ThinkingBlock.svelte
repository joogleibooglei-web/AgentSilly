<script lang="ts">
  interface Props {
    content: string;
    isActive?: boolean;
  }

  let { content, isActive = false }: Props = $props();
  let expanded = $state(false);

  function toggle() {
    expanded = !expanded;
  }
</script>

<div class="thinking-block">
  <button class="thinking-toggle" class:open={expanded} onclick={toggle}>
    {#if isActive}
      <div class="thinking-spinner"></div>
    {:else}
      <div class="thinking-dot">💭</div>
    {/if}
    <span>{isActive ? 'Thinking...' : 'Thought process'}</span>
    <span class="chevron">▶</span>
  </button>
  {#if expanded}
    <div class="thinking-content">
      {content}
    </div>
  {/if}
</div>

<style>
  .thinking-block {
    align-self: stretch;
    animation: fadeIn 200ms ease;
  }

  .thinking-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 4px;
    cursor: pointer;
    font-size: 11px;
    color: var(--text-muted);
    transition: all 120ms;
    width: 100%;
    font-family: inherit;
  }

  .thinking-toggle:hover {
    background: var(--surface-hover);
    color: var(--text-secondary);
  }

  .thinking-toggle .chevron {
    transition: transform 200ms;
    font-size: 9px;
    margin-left: auto;
  }

  .thinking-toggle.open .chevron {
    transform: rotate(90deg);
  }

  .thinking-spinner {
    width: 10px;
    height: 10px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .thinking-dot {
    font-size: 10px;
    line-height: 1;
  }

  .thinking-content {
    padding: 8px 10px;
    margin-top: 4px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-muted);
    font-family: var(--mono);
    max-height: 150px;
    overflow-y: auto;
    white-space: pre-wrap;
  }

  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: translateY(0); }
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
