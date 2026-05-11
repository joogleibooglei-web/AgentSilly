/**
 * WebSocket client for communicating with the ENI sidecar.
 *
 * Features:
 * - Connect to sidecar on configurable port (default 7842)
 * - Auto-reconnect with exponential backoff on disconnect
 * - Parse incoming ServerMessage JSON and dispatch to stores
 * - Send ClientMessage JSON
 */
import {
  setConnected,
  setDisconnected,
  setReconnecting,
  setSetupRequired,
} from '../stores/connection';
import {
  appendToken,
  appendThinking,
  completeMessage,
  addSystemMessage,
  setToolStart,
  setToolEnd,
  finalizeStreaming,
  setMessages,
  type Message,
} from '../stores/conversation';
import {
  setAgentStatus,
  setPreviewData,
  setUndoAvailable,
  type RightPaneTab,
} from '../stores/ui';
import { setActiveModel, setModelProfiles, setPostCardPrompt, setStBaseUrl, updateConfig, type ModelProfile } from '../stores/config';

// --- Protocol Types ---

export type ClientMessage =
  | { type: 'user_message'; content: string }
  | { type: 'cancel' }
  | { type: 'switch_model'; profile: string }
  | { type: 'new_conversation' }
  | { type: 'undo'; entity_type: string; entity_id: string }
  | { type: 'update_config'; key: string; value: unknown }
  | { type: 'register_session'; session_id: string };

export type ServerMessage =
  | { type: 'token'; content: string }
  | { type: 'thinking'; content: string }
  | { type: 'message_complete'; id: string }
  | { type: 'tool_start'; name: string; description: string }
  | { type: 'tool_end'; name: string; success: boolean }
  | { type: 'preview'; tab: 'character' | 'world' | 'posthistory' | 'persona'; data: unknown }
  | { type: 'error'; message: string }
  | { type: 'status'; state: 'idle' | 'thinking' | 'tool_executing' }
  | { type: 'undo_available'; entity_type: string; entity_id: string; summary: string }
  | { type: 'system_message'; content: string }
  | { type: 'config_updated'; key: string };

// --- Client Configuration ---

export interface WsClientOptions {
  port?: number;
  httpPort?: number;
  host?: string;
  maxReconnectAttempts?: number;
  baseReconnectDelay?: number;
  maxReconnectDelay?: number;
}

const DEFAULT_OPTIONS: Required<WsClientOptions> = {
  port: 7842,
  httpPort: 7843,
  host: '127.0.0.1',
  maxReconnectAttempts: 10,
  baseReconnectDelay: 1000,
  maxReconnectDelay: 30000,
};

// --- WebSocket Client ---

/**
 * Get or create a unique session ID for this browser tab.
 * Uses sessionStorage so each tab gets its own UUID that persists
 * across page reloads within the same tab, but is unique per tab.
 */
function getTabSessionId(): string {
  const STORAGE_KEY = 'eni_tab_session_id';
  let sessionId = sessionStorage.getItem(STORAGE_KEY);
  if (!sessionId) {
    sessionId = crypto.randomUUID();
    sessionStorage.setItem(STORAGE_KEY, sessionId);
  }
  return sessionId;
}

export class WsClient {
  private ws: WebSocket | null = null;
  private options: Required<WsClientOptions>;
  private reconnectAttempts = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private intentionalClose = false;

  constructor(options: WsClientOptions = {}) {
    this.options = { ...DEFAULT_OPTIONS, ...options };
  }

  /**
   * Connect to the sidecar WebSocket server.
   */
  connect(): void {
    this.intentionalClose = false;
    const url = `ws://${this.options.host}:${this.options.port}`;

    try {
      this.ws = new WebSocket(url);
    } catch {
      this.handleDisconnect();
      return;
    }

    this.ws.onopen = () => {
      this.reconnectAttempts = 0;
      setConnected();
      // Register this tab's session ID so the sidecar ties the conversation to this tab
      this.send({ type: 'register_session', session_id: getTabSessionId() });
      this.reportStUrl();
      this.fetchConfig();
      this.fetchConversationHistory();
    };

    this.ws.onmessage = (event: MessageEvent) => {
      this.handleMessage(event.data);
    };

    this.ws.onclose = () => {
      if (!this.intentionalClose) {
        this.handleDisconnect();
      }
    };

    this.ws.onerror = () => {
      // onclose will fire after onerror, so reconnect logic is handled there
    };
  }

  /**
   * Disconnect from the sidecar.
   */
  disconnect(): void {
    this.intentionalClose = true;
    this.clearReconnectTimer();
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    setDisconnected();
  }

  /**
   * Send a ClientMessage to the sidecar.
   */
  send(message: ClientMessage): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      console.warn('[WsClient] Cannot send — not connected');
      return;
    }
    this.ws.send(JSON.stringify(message));
  }

  /**
   * Send a user message.
   */
  sendUserMessage(content: string): void {
    this.send({ type: 'user_message', content });
  }

  /**
   * Send a cancel signal.
   */
  sendCancel(): void {
    this.send({ type: 'cancel' });
  }

  /**
   * Send a model switch request.
   */
  sendSwitchModel(profile: string): void {
    this.send({ type: 'switch_model', profile });
  }

  /**
   * Send a new conversation request.
   * Also resets the tab session ID so the new conversation gets a fresh UUID.
   */
  sendNewConversation(): void {
    this.send({ type: 'new_conversation' });
    // Generate a new session ID for this tab's new conversation
    const newSessionId = crypto.randomUUID();
    sessionStorage.setItem('eni_tab_session_id', newSessionId);
    // Register the new session with the sidecar
    this.send({ type: 'register_session', session_id: newSessionId });
  }

  /**
   * Send an undo request.
   */
  sendUndo(entityType: string, entityId: string): void {
    this.send({ type: 'undo', entity_type: entityType, entity_id: entityId });
  }

  /**
   * Check if the client is currently connected.
   */
  get isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  /**
   * Fetch model profiles and config from the sidecar HTTP API on connect.
   * Populates the config store with available model profiles.
   */
  private async fetchConfig(): Promise<void> {
    const url = `http://${this.options.host}:${this.options.httpPort}/config`;
    try {
      const response = await fetch(url);
      if (!response.ok) {
        console.warn('[WsClient] Failed to fetch config:', response.status);
        return;
      }
      const data = await response.json();
      if (data.model_profiles && Array.isArray(data.model_profiles)) {
        const profiles: ModelProfile[] = data.model_profiles.map((p: Record<string, unknown>) => ({
          id: (p.name as string) || (p.id as string),
          name: p.name as string,
          baseUrl: p.base_url as string,
          apiKey: '', // API key is not exposed via GET /config for security
          model: p.model as string,
          temperature: p.temperature as number,
          maxTokens: p.max_tokens as number,
          isDefault: p.is_default as boolean,
        }));
        setModelProfiles(profiles);
      }

      // If there's an active user-configured profile from the DB, use it
      if (data.active_profile) {
        const ap = data.active_profile;
        const activeProfile: ModelProfile = {
          id: 'user-configured',
          name: 'User Configured',
          baseUrl: ap.base_url || '',
          apiKey: ap.api_key || '',
          model: ap.model || '',
          temperature: ap.temperature ?? 0.7,
          maxTokens: ap.max_tokens ?? 4096,
          isDefault: true,
        };
        setModelProfiles([activeProfile]);
        setActiveModel('user-configured');
      }

      // Store the SillyTavern base URL for avatar image loading
      if (data.st_base_url) {
        setStBaseUrl(data.st_base_url);
      }

      // Hydrate the post-card prompt from persisted config
      if (data.post_card_prompt) {
        setPostCardPrompt(data.post_card_prompt);
      }
    } catch (e) {
      console.warn('[WsClient] Failed to fetch config from sidecar:', e);
    }
  }

  /**
   * Fetch the conversation history for this tab's session from the sidecar HTTP API.
   * Uses the tab session ID as the conversation ID.
   */
  private async fetchConversationHistory(): Promise<void> {
    const baseUrl = `http://${this.options.host}:${this.options.httpPort}`;
    const sessionId = getTabSessionId();
    try {
      // Fetch messages for this tab's conversation (session_id == conversation_id)
      const messagesResponse = await fetch(`${baseUrl}/conversations/${sessionId}`);
      if (!messagesResponse.ok) {
        // 404 means no conversation yet for this tab — that's fine, start fresh
        return;
      }
      const data = await messagesResponse.json();
      const rawMessages: Array<{
        id: string;
        role: string;
        content: string;
        created_at?: string;
        metadata?: string;
      }> = data.messages || data;

      if (!rawMessages || rawMessages.length === 0) {
        return;
      }

      // Map raw messages to the frontend Message format.
      // Filter out internal messages that shouldn't be displayed:
      // - tool/tool_call/tool_result roles (internal agent loop messages)
      // - post-tool [System: ...] instructions injected as user role
      // - empty-content assistant messages (tool call placeholders)
      const restoredMessages: Message[] = rawMessages
        .filter((m) => {
          // Only keep user, assistant, and system roles
          if (m.role !== 'user' && m.role !== 'assistant' && m.role !== 'system') {
            return false;
          }
          // Filter out empty content messages (tool call assistant placeholders)
          if (!m.content || m.content.trim() === '') {
            return false;
          }
          // Filter out internal post-tool instructions injected as user messages
          if (m.content.trimStart().startsWith('[System:')) {
            return false;
          }
          return true;
        })
        .map((m) => ({
          id: m.id || crypto.randomUUID(),
          role: m.role as Message['role'],
          content: m.content,
          timestamp: m.created_at ? new Date(m.created_at).getTime() : Date.now(),
        }));

      if (restoredMessages.length > 0) {
        setMessages(restoredMessages);
      }
    } catch (e) {
      console.warn('[WsClient] Failed to fetch conversation history from sidecar:', e);
    }
  }

  // --- Private Methods ---

  /**
   * Report the SillyTavern origin URL to the sidecar.
   * Since this extension runs inside ST's browser context, window.location.origin
   * gives us the correct ST URL automatically.
   */
  private reportStUrl(): void {
    if (typeof window !== 'undefined' && window.location) {
      const stUrl = window.location.origin;
      // Only report if it looks like a local ST instance (not file:// or about:)
      if (stUrl.startsWith('http')) {
        this.send({ type: 'report_st_url', url: stUrl } as any);
      }
    }
  }

  private handleMessage(data: string): void {
    let msg: ServerMessage;
    try {
      msg = JSON.parse(data) as ServerMessage;
    } catch {
      console.warn('[WsClient] Failed to parse message:', data);
      return;
    }

    this.dispatchMessage(msg);
  }

  private dispatchMessage(msg: ServerMessage): void {
    switch (msg.type) {
      case 'token':
        appendToken(msg.content);
        break;

      case 'thinking':
        appendThinking(msg.content);
        break;

      case 'message_complete':
        completeMessage(msg.id);
        break;

      case 'tool_start':
        setToolStart(msg.name, msg.description);
        break;

      case 'tool_end':
        setToolEnd(msg.name, msg.success);
        break;

      case 'preview':
        setPreviewData(msg.tab as RightPaneTab, msg.data);
        break;

      case 'error':
        addSystemMessage(`Error: ${msg.message}`);
        break;

      case 'status':
        setAgentStatus(msg.state);
        // When agent returns to idle (e.g., after cancellation), finalize any in-progress streaming
        if (msg.state === 'idle') {
          finalizeStreaming();
        }
        break;

      case 'undo_available':
        setUndoAvailable({
          entityType: msg.entity_type,
          entityId: msg.entity_id,
          summary: msg.summary,
        });
        break;

      case 'system_message':
        addSystemMessage(msg.content);
        break;

      case 'config_updated':
        if (msg.key === 'model_profile') {
          // Model profile changed externally — re-fetch config to sync state
          this.fetchConfig();
        } else if (msg.key === 'model_switched') {
          // Model switch confirmed by sidecar — do NOT re-fetch (would overwrite user's selection)
        } else {
          updateConfig(msg.key, null);
        }
        break;

      default:
        console.warn('[WsClient] Unknown message type:', (msg as { type: string }).type);
    }
  }

  private handleDisconnect(): void {
    this.ws = null;
    setDisconnected();

    if (this.reconnectAttempts >= this.options.maxReconnectAttempts) {
      setSetupRequired();
      addSystemMessage(
        'Unable to connect to the sidecar. Please check that it is running.'
      );
      return;
    }

    setReconnecting();
    this.scheduleReconnect();
  }

  private scheduleReconnect(): void {
    const delay = Math.min(
      this.options.baseReconnectDelay * Math.pow(2, this.reconnectAttempts),
      this.options.maxReconnectDelay
    );

    this.reconnectTimer = setTimeout(() => {
      this.reconnectAttempts++;
      this.connect();
    }, delay);
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }
}

// Singleton instance
let clientInstance: WsClient | null = null;

/**
 * Get or create the singleton WebSocket client.
 */
export function getWsClient(options?: WsClientOptions): WsClient {
  if (!clientInstance) {
    clientInstance = new WsClient(options);
  }
  return clientInstance;
}

/**
 * Reset the singleton (for testing or reconfiguration).
 */
export function resetWsClient(): void {
  if (clientInstance) {
    clientInstance.disconnect();
    clientInstance = null;
  }
}
