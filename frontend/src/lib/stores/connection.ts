/**
 * WebSocket connection state store.
 *
 * Tracks the connection status between the frontend and the Rust sidecar.
 */
import { writable } from 'svelte/store';

export type ConnectionState = 'connected' | 'disconnected' | 'reconnecting';

export interface ConnectionStore {
  state: ConnectionState;
  port: number;
  reconnectAttempts: number;
}

const initialState: ConnectionStore = {
  state: 'disconnected',
  port: 7842,
  reconnectAttempts: 0,
};

export const connection = writable<ConnectionStore>(initialState);

export function setConnected(): void {
  connection.update((s) => ({ ...s, state: 'connected', reconnectAttempts: 0 }));
}

export function setDisconnected(): void {
  connection.update((s) => ({ ...s, state: 'disconnected' }));
}

export function setReconnecting(): void {
  connection.update((s) => ({
    ...s,
    state: 'reconnecting',
    reconnectAttempts: s.reconnectAttempts + 1,
  }));
}

export function setPort(port: number): void {
  connection.update((s) => ({ ...s, port }));
}
