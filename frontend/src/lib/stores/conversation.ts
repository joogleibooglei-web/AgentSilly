/**
 * Conversation store.
 *
 * Manages the messages array, streaming state, and current streaming content.
 */
import { writable, derived } from 'svelte/store';

export type MessageRole = 'user' | 'assistant' | 'system';

export interface ToolCallInfo {
  name: string;
  description: string;
  success?: boolean;
}

export interface Message {
  id: string;
  role: MessageRole;
  content: string;
  timestamp: number;
  toolCall?: ToolCallInfo;
  thinking?: string;
}

export interface ConversationStore {
  messages: Message[];
  isStreaming: boolean;
  streamingContent: string;
  streamingThinking: string;
  activeToolCall: ToolCallInfo | null;
}

const initialState: ConversationStore = {
  messages: [],
  isStreaming: false,
  streamingContent: '',
  streamingThinking: '',
  activeToolCall: null,
};

export const conversation = writable<ConversationStore>(initialState);

export const messages = derived(conversation, ($c) => $c.messages);
export const isStreaming = derived(conversation, ($c) => $c.isStreaming);

export function appendToken(content: string): void {
  conversation.update((s) => ({
    ...s,
    isStreaming: true,
    streamingContent: s.streamingContent + content,
  }));
}

export function appendThinking(content: string): void {
  conversation.update((s) => ({
    ...s,
    streamingThinking: s.streamingThinking + content,
  }));
}

export function completeMessage(id: string): void {
  conversation.update((s) => {
    const assistantMessage: Message = {
      id,
      role: 'assistant',
      content: s.streamingContent,
      timestamp: Date.now(),
      thinking: s.streamingThinking || undefined,
    };
    return {
      ...s,
      messages: [...s.messages, assistantMessage],
      isStreaming: false,
      streamingContent: '',
      streamingThinking: '',
      activeToolCall: null,
    };
  });
}

export function addUserMessage(content: string): void {
  const msg: Message = {
    id: crypto.randomUUID(),
    role: 'user',
    content,
    timestamp: Date.now(),
  };
  conversation.update((s) => ({
    ...s,
    messages: [...s.messages, msg],
  }));
}

export function addSystemMessage(content: string): void {
  const msg: Message = {
    id: crypto.randomUUID(),
    role: 'system',
    content,
    timestamp: Date.now(),
  };
  conversation.update((s) => ({
    ...s,
    messages: [...s.messages, msg],
  }));
}

export function setToolStart(name: string, description: string): void {
  conversation.update((s) => ({
    ...s,
    activeToolCall: { name, description },
  }));
}

export function setToolEnd(name: string, success: boolean): void {
  conversation.update((s) => {
    const toolMsg: Message = {
      id: crypto.randomUUID(),
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      toolCall: { name, description: s.activeToolCall?.description ?? '', success },
    };
    return {
      ...s,
      messages: [...s.messages, toolMsg],
      activeToolCall: null,
    };
  });
}

export function clearConversation(): void {
  conversation.set(initialState);
}

export function setMessages(msgs: Message[]): void {
  conversation.update((s) => ({ ...s, messages: msgs }));
}
