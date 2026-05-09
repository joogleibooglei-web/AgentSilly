<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import { conversation, addUserMessage, clearConversation, type Message } from '../lib/stores/conversation';
  import { ui, type AgentStatus } from '../lib/stores/ui';
  import { connection, type ConnectionState, type PlatformInfo } from '../lib/stores/connection';
  import { getWsClient } from '../lib/ws/client';
  import MessageBubble from './MessageBubble.svelte';
  import ToolCallCard from './ToolCallCard.svelte';
  import ThinkingBlock from './ThinkingBlock.svelte';
  import SetupGuide from './SetupGuide.svelte';

  let messages: Message[] = $state([]);
  let isStreaming = $state(false);
  let streamingContent = $state('');
  let streamingThinking = $state('');
  let activeToolCall: { name: string; description: string; success?: boolean } | null = $state(null);
  let agentStatus: AgentStatus = $state('idle');
  let connectionState: ConnectionState = $state('disconnected');
  let platformInfo: PlatformInfo = $state({ platform: 'unknown', arch: 'x64' });

  let inputValue = $state('');
  let messagesEl: HTMLDivElement | undefined = $state(undefined);
  let textareaEl: HTMLTextAreaElement | undefined = $state(undefined);

  const unsubConversation = conversation.subscribe(($c) => {
    messages = $c.messages;
    isStreaming = $c.isStreaming;
    streamingContent = $c.streamingContent;
    streamingThinking = $c.streamingThinking;
    activeToolCall = $c.activeToolCall;
    scrollToBottom();
  });

  const unsubUi = ui.subscribe(($ui) => {
    agentStatus = $ui.agentStatus;
  });

  const unsubConnection = connection.subscribe(($conn) => {
    connectionState = $conn.state;
    platformInfo = $conn.platformInfo;
  });

  async function scrollToBottom() {
    await tick();
    if (messagesEl) {
      messagesEl.scrollTop = messagesEl.scrollHeight;
    }
  }

  function getStatusText(status: AgentStatus): string {
    switch (status) {
      case 'idle': return 'Ready to help';
      case 'thinking': return 'Thinking...';
      case 'tool_executing': return 'Executing tool...';
      default: return 'Ready to help';
    }
  }

  function handleSend() {
    const content = inputValue.trim();
    if (!content) return;

    addUserMessage(content);
    const wsClient = getWsClient();
    wsClient.sendUserMessage(content);
    inputValue = '';
    resizeTextarea();
  }

  function handleStop() {
    const wsClient = getWsClient();
    wsClient.sendCancel();
  }

  function handleNewChat() {
    const wsClient = getWsClient();
    wsClient.sendNewConversation();
    clearConversation();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      if (isStreaming) return;
      handleSend();
    }
  }

  function handleInput() {
    resizeTextarea();
  }

  function resizeTextarea() {
    if (!textareaEl) return;
    textareaEl.style.height = 'auto';
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 120) + 'px';
  }

  onDestroy(() => {
    unsubConversation();
    unsubUi();
    unsubConnection();
  });
</script>

<div class="chat-pane">
  <!-- Status bar -->
  <div class="eni-status">
    <div class="eni-avatar">
      E
      <div class="status-dot" class:active={agentStatus === 'idle'} class:thinking={agentStatus !== 'idle'}></div>
    </div>
    <div class="eni-info">
      <span class="eni-name">ENI</span>
      <span class="eni-state">{getStatusText(agentStatus)}</span>
    </div>
    <button
      class="new-chat-btn"
      onclick={handleNewChat}
      disabled={connectionState !== 'connected' || isStreaming}
      title="Start a new conversation"
    >New Chat</button>
  </div>

  <!-- Reconnection banner -->
  {#if connectionState === 'reconnecting'}
    <div class="reconnect-banner">
      <span class="reconnect-dot"></span>
      <span class="reconnect-text">Reconnecting to sidecar...</span>
    </div>
  {:else if connectionState === 'disconnected' && !isStreaming}
    <div class="disconnected-banner">
      <span class="disconnected-text">Disconnected from sidecar</span>
    </div>
  {/if}

  <!-- Messages area -->
  <div class="messages" bind:this={messagesEl}>
    {#if connectionState === 'setup_required'}
      <SetupGuide {platformInfo} />
    {:else if messages.length === 0 && !isStreaming}
      <div class="welcome-message">
        <div class="message assistant">
          <div class="msg-bubble">Hey! I'm ENI, your world-building assistant. What are we working on today?</div>
        </div>
      </div>
    {/if}

    {#each messages as message (message.id)}
      {#if message.toolCall}
        <ToolCallCard
          name={message.toolCall.name}
          description={message.toolCall.description}
          success={message.toolCall.success}
        />
      {:else if message.thinking}
        <ThinkingBlock content={message.thinking} />
        <MessageBubble {message} />
      {:else}
        <MessageBubble {message} />
      {/if}
    {/each}

    <!-- Active tool call indicator -->
    {#if activeToolCall}
      <ToolCallCard
        name={activeToolCall.name}
        description={activeToolCall.description}
        isActive={true}
      />
    {/if}

    <!-- Streaming thinking block -->
    {#if streamingThinking}
      <ThinkingBlock content={streamingThinking} isActive={true} />
    {/if}

    <!-- Streaming message -->
    {#if isStreaming && streamingContent}
      <div class="message assistant">
        <div class="msg-bubble streaming">{streamingContent}<span class="cursor">▊</span></div>
      </div>
    {/if}
  </div>

  <!-- Input area -->
  <div class="input-area">
    <div class="input-row">
      <textarea
        class="input-field"
        placeholder={connectionState === 'connected' ? 'Message ENI...' : 'Waiting for connection...'}
        rows="1"
        bind:this={textareaEl}
        bind:value={inputValue}
        onkeydown={handleKeydown}
        oninput={handleInput}
        disabled={isStreaming || connectionState !== 'connected'}
      ></textarea>
      {#if isStreaming}
        <button class="stop-btn" onclick={handleStop}>Stop</button>
      {:else}
        <button class="send-btn" onclick={handleSend} disabled={!inputValue.trim() || connectionState !== 'connected'}>Send</button>
      {/if}
    </div>
    <div class="input-hint">Enter to send · Shift+Enter for new line</div>
  </div>
</div>

<style>
  .chat-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
  }

  /* Status bar */
  .eni-status {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-elevated);
    flex-shrink: 0;
  }

  .eni-avatar {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--accent);
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 12px;
    color: #1b1b1b;
    position: relative;
    flex-shrink: 0;
  }

  .status-dot {
    position: absolute;
    bottom: -1px;
    right: -1px;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--text-muted);
    border: 2px solid var(--bg-elevated);
  }

  .status-dot.active {
    background: var(--success);
  }

  .status-dot.thinking {
    background: var(--warning);
    animation: pulse 1s ease-in-out infinite;
  }

  .eni-info {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  .eni-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
  }

  .eni-state {
    font-size: 10px;
    color: var(--text-muted);
  }

  /* New Chat button */
  .new-chat-btn {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 10px;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    transition: background 120ms, color 120ms, border-color 120ms;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .new-chat-btn:hover:not(:disabled) {
    background: var(--bg-surface);
    color: var(--text);
    border-color: var(--accent);
  }

  .new-chat-btn:active:not(:disabled) {
    transform: scale(0.96);
  }

  .new-chat-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  /* Messages */
  .messages {
    flex: 1;
    overflow-y: auto;
    padding: 16px 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .welcome-message {
    display: contents;
  }

  .message {
    display: flex;
    flex-direction: column;
    max-width: 90%;
    animation: fadeIn 200ms ease;
  }

  .message.assistant {
    align-self: flex-start;
  }

  .msg-bubble {
    padding: 9px 13px;
    border-radius: 6px;
    font-size: 12.5px;
    line-height: 1.55;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    color: var(--text);
    word-wrap: break-word;
    overflow-wrap: break-word;
    white-space: pre-wrap;
  }

  .msg-bubble.streaming {
    border-color: var(--accent);
  }

  .cursor {
    animation: blink 1s step-end infinite;
    color: var(--accent);
  }

  /* Input area */
  .input-area {
    padding: 12px 14px;
    border-top: 1px solid var(--border);
    background: var(--bg-elevated);
    flex-shrink: 0;
  }

  .input-row {
    display: flex;
    gap: 8px;
    align-items: flex-end;
  }

  .input-field {
    flex: 1;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 9px 12px;
    color: var(--text);
    font-size: 12.5px;
    font-family: inherit;
    outline: none;
    resize: none;
    min-height: 38px;
    max-height: 120px;
    transition: border-color 150ms;
  }

  .input-field:focus {
    border-color: var(--accent);
  }

  .input-field::placeholder {
    color: var(--text-muted);
  }

  .input-field:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .send-btn {
    background: var(--accent);
    border: none;
    border-radius: 6px;
    padding: 9px 16px;
    color: #1b1b1b;
    font-size: 12px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    transition: background 120ms, transform 80ms;
    white-space: nowrap;
  }

  .send-btn:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  .send-btn:active:not(:disabled) {
    transform: scale(0.96);
  }

  .send-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .stop-btn {
    background: var(--error);
    border: none;
    border-radius: 6px;
    padding: 9px 16px;
    color: white;
    font-size: 12px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    transition: background 120ms, transform 80ms;
    white-space: nowrap;
  }

  .stop-btn:hover {
    background: #e53935;
  }

  .stop-btn:active {
    transform: scale(0.96);
  }

  .input-hint {
    font-size: 10px;
    color: var(--text-muted);
    margin-top: 5px;
  }

  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: translateY(0); }
  }

  @keyframes blink {
    50% { opacity: 0; }
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  /* Reconnection banner */
  .reconnect-banner {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 6px 14px;
    background: var(--warning-bg, rgba(255, 152, 0, 0.1));
    border-bottom: 1px solid var(--warning);
    flex-shrink: 0;
    animation: fadeIn 200ms ease;
  }

  .reconnect-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--warning);
    animation: pulse 1s ease-in-out infinite;
  }

  .reconnect-text {
    font-size: 11px;
    font-weight: 500;
    color: var(--warning);
  }

  /* Disconnected banner */
  .disconnected-banner {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 5px 14px;
    background: var(--error-bg, rgba(244, 67, 54, 0.08));
    border-bottom: 1px solid var(--error);
    flex-shrink: 0;
    animation: fadeIn 200ms ease;
  }

  .disconnected-text {
    font-size: 11px;
    font-weight: 500;
    color: var(--error);
  }
</style>
