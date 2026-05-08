<script lang="ts">
  import type { Message } from '../lib/stores/conversation';

  interface Props {
    message: Message;
  }

  let { message }: Props = $props();

  /**
   * Simple markdown rendering: bold, italic, inline code, code blocks, lists.
   * Returns sanitized HTML string.
   */
  function renderMarkdown(text: string): string {
    let html = escapeHtml(text);

    // Code blocks (```...```)
    html = html.replace(/```(\w*)\n([\s\S]*?)```/g, (_match, _lang, code) => {
      return `<pre class="code-block"><code>${code.trim()}</code></pre>`;
    });

    // Inline code (`...`)
    html = html.replace(/`([^`]+)`/g, '<code class="inline-code">$1</code>');

    // Bold (**...**)
    html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');

    // Italic (*...*)
    html = html.replace(/\*([^*]+)\*/g, '<em>$1</em>');

    // Unordered lists (- item)
    html = html.replace(/^- (.+)$/gm, '<li>$1</li>');
    html = html.replace(/(<li>.*<\/li>\n?)+/g, (match) => `<ul>${match}</ul>`);

    // Line breaks
    html = html.replace(/\n/g, '<br>');

    return html;
  }

  function escapeHtml(text: string): string {
    return text
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }
</script>

<div class="message" class:user={message.role === 'user'} class:assistant={message.role === 'assistant'} class:system={message.role === 'system'}>
  <div class="msg-bubble">
    {#if message.role === 'system'}
      <span class="system-label">System</span>
    {/if}
    {@html renderMarkdown(message.content)}
  </div>
</div>

<style>
  .message {
    display: flex;
    flex-direction: column;
    max-width: 90%;
    animation: fadeIn 200ms ease;
  }

  .message.user {
    align-self: flex-end;
  }

  .message.assistant {
    align-self: flex-start;
  }

  .message.system {
    align-self: center;
    max-width: 80%;
  }

  .msg-bubble {
    padding: 9px 13px;
    border-radius: 6px;
    font-size: 12.5px;
    line-height: 1.55;
    word-wrap: break-word;
    overflow-wrap: break-word;
  }

  .message.assistant .msg-bubble {
    background: var(--bg-surface, #1f1f36);
    border: 1px solid var(--border, #3a3a5c);
    color: var(--text, #e0e0e0);
  }

  .message.user .msg-bubble {
    background: rgba(124, 92, 252, 0.12);
    border: 1px solid rgba(124, 92, 252, 0.3);
    color: var(--text, #e0e0e0);
  }

  .message.system .msg-bubble {
    background: transparent;
    border: 1px solid var(--border, #3a3a5c);
    color: var(--text-muted, #6b6b8a);
    font-size: 11px;
    text-align: center;
    font-style: italic;
  }

  .system-label {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted, #6b6b8a);
    display: block;
    margin-bottom: 4px;
  }

  .msg-bubble :global(strong) {
    font-weight: 600;
  }

  .msg-bubble :global(em) {
    font-style: italic;
  }

  .msg-bubble :global(.inline-code) {
    background: var(--bg-elevated, #252542);
    padding: 1px 4px;
    border-radius: 3px;
    font-family: var(--mono, 'JetBrains Mono', 'Fira Code', monospace);
    font-size: 11px;
  }

  .msg-bubble :global(.code-block) {
    background: var(--bg-elevated, #252542);
    border: 1px solid var(--border, #3a3a5c);
    border-radius: 4px;
    padding: 8px 10px;
    margin: 6px 0;
    overflow-x: auto;
    font-family: var(--mono, 'JetBrains Mono', 'Fira Code', monospace);
    font-size: 11px;
    line-height: 1.4;
  }

  .msg-bubble :global(ul) {
    padding-left: 16px;
    margin: 4px 0;
  }

  .msg-bubble :global(li) {
    margin: 2px 0;
  }

  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: translateY(0); }
  }
</style>
