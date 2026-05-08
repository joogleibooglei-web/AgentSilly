/**
 * WebSocket connection state store.
 *
 * Tracks the connection status between the frontend and the Rust sidecar.
 */
import { writable } from 'svelte/store';

export type ConnectionState = 'connected' | 'disconnected' | 'reconnecting' | 'setup_required';

export interface PlatformInfo {
  platform: string;
  arch: string;
}

export interface ConnectionStore {
  state: ConnectionState;
  port: number;
  reconnectAttempts: number;
  platformInfo: PlatformInfo;
}

/**
 * Detect the current platform and architecture from the user agent string.
 * Falls back to 'unknown' if detection fails.
 */
function detectPlatform(): PlatformInfo {
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent.toLowerCase() : '';

  let platform = 'unknown';
  if (ua.includes('mac') || ua.includes('darwin')) {
    platform = 'darwin';
  } else if (ua.includes('win')) {
    platform = 'win32';
  } else if (ua.includes('linux')) {
    platform = 'linux';
  }

  let arch = 'x64';
  if (ua.includes('arm64') || ua.includes('aarch64')) {
    arch = 'arm64';
  }

  return { platform, arch };
}

const initialState: ConnectionStore = {
  state: 'disconnected',
  port: 7842,
  reconnectAttempts: 0,
  platformInfo: detectPlatform(),
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

export function setSetupRequired(): void {
  connection.update((s) => ({ ...s, state: 'setup_required' }));
}

export function setPort(port: number): void {
  connection.update((s) => ({ ...s, port }));
}
