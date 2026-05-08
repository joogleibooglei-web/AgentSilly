/**
 * Configuration store.
 *
 * Manages model profiles, active model, and post-card prompt.
 */
import { writable, derived } from 'svelte/store';

export interface ModelProfile {
  id: string;
  name: string;
  baseUrl: string;
  apiKey: string;
  model: string;
  temperature: number;
  maxTokens: number;
  isDefault: boolean;
}

export interface ConfigStore {
  modelProfiles: ModelProfile[];
  activeModelId: string | null;
  postCardPrompt: string;
}

const initialState: ConfigStore = {
  modelProfiles: [],
  activeModelId: null,
  postCardPrompt: '',
};

export const config = writable<ConfigStore>(initialState);

export const activeModel = derived(config, ($c) =>
  $c.modelProfiles.find((p) => p.id === $c.activeModelId) ?? null
);

export const modelProfiles = derived(config, ($c) => $c.modelProfiles);

export function setModelProfiles(profiles: ModelProfile[]): void {
  config.update((s) => {
    const activeId = s.activeModelId ?? profiles.find((p) => p.isDefault)?.id ?? profiles[0]?.id ?? null;
    return { ...s, modelProfiles: profiles, activeModelId: activeId };
  });
}

export function setActiveModel(profileId: string): void {
  config.update((s) => ({ ...s, activeModelId: profileId }));
}

export function setPostCardPrompt(prompt: string): void {
  config.update((s) => ({ ...s, postCardPrompt: prompt }));
}

export function updateConfig(key: string, value: unknown): void {
  config.update((s) => {
    if (key === 'postCardPrompt' && typeof value === 'string') {
      return { ...s, postCardPrompt: value };
    }
    return s;
  });
}
