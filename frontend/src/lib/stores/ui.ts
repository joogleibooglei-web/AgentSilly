/**
 * UI state store.
 *
 * Manages panel mode, active right-pane tab, and preview data.
 */
import { writable, derived } from 'svelte/store';

export type PanelMode = 'chat-only' | 'split';
export type RightPaneTab = 'character' | 'world' | 'posthistory' | 'persona' | 'settings';
export type AgentStatus = 'idle' | 'thinking' | 'tool_executing';

export interface UndoInfo {
  entityType: string;
  entityId: string;
  summary: string;
}

export interface UiStore {
  panelMode: PanelMode;
  activeTab: RightPaneTab;
  previewData: Record<RightPaneTab, unknown>;
  agentStatus: AgentStatus;
  undoAvailable: UndoInfo | null;
}

const initialState: UiStore = {
  panelMode: 'chat-only',
  activeTab: 'character',
  previewData: {
    character: null,
    world: null,
    posthistory: null,
    persona: null,
    settings: null,
  },
  agentStatus: 'idle',
  undoAvailable: null,
};

export const ui = writable<UiStore>(initialState);

export const panelMode = derived(ui, ($ui) => $ui.panelMode);
export const activeTab = derived(ui, ($ui) => $ui.activeTab);
export const agentStatus = derived(ui, ($ui) => $ui.agentStatus);

export function openRightPane(tab: RightPaneTab): void {
  ui.update((s) => ({ ...s, panelMode: 'split', activeTab: tab }));
}

export function closeRightPane(): void {
  ui.update((s) => ({ ...s, panelMode: 'chat-only' }));
}

export function switchTab(tab: RightPaneTab): void {
  ui.update((s) => ({ ...s, activeTab: tab }));
}

export function setPreviewData(tab: RightPaneTab, data: unknown): void {
  ui.update((s) => ({
    ...s,
    panelMode: 'split',
    activeTab: tab,
    previewData: { ...s.previewData, [tab]: data },
  }));
}

export function setAgentStatus(status: AgentStatus): void {
  ui.update((s) => ({ ...s, agentStatus: status }));
}

export function setUndoAvailable(info: UndoInfo | null): void {
  ui.update((s) => ({ ...s, undoAvailable: info }));
}
