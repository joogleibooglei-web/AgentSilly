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
} from '../stores/connection';
import {
  appendToken,
  appendThinking,
  completeMessage,
  addSystemMessage,
  setToolStart,
  setToolEnd,
} from '../stores/conversation';
import {
  setAgentStatus,
  setPreviewData,
  setUndoAvailable,
  type RightPaneTab,
} from '../stores/ui';
import { setActiveModel, setModelProfiles, updateConfig } from '../stores/config';

// --- Protocol Types ---

export type ClientMessage =
  | { type: 'user_message'; content: string }
  | { type: 'cancel' }
  | { type: 'switch_model'; profile: string }
  | { type: 'new_conversation' }
  | { type: 'undo'; entity_type: string; entity_id: string }
  | { type: 'update_config'; key: string; value: unknown };

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
  host?: string;
  maxReconnectAttempts?: number;
  baseReconnectDelay?: number;
  maxReconnectDelay?: number;
}

const DEFAULT_OPTIONS: Required<WsClientOptions> = {
  port: 7842,
  host: '127.0.0.1',
  maxReconnectAttempts: 10,
  baseReconnectDelay: 1000,
  maxReconnectDelay: 30000,
};

// --- WebSocket Client ---

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
   */
  sendNewConversation(): void {
    this.send({ type: 'new_conversation' });
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

  // --- Private Methods ---

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
        updateConfig(msg.key, null);
        break;

      default:
        console.warn('[WsClient] Unknown message type:', (msg as { type: string }).type);
    }
  }

  private handleDisconnect(): void {
    this.ws = null;
    setDisconnected();

    if (this.reconnectAttempts >= this.options.maxReconnectAttempts) {
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
