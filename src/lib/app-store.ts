// app-store.ts — key/value seam for small persisted state.
//
// String keys, JSON-encoded values. Reads return null and writes no-op when
// storage is unavailable (private mode), so callers need no try/catch of
// their own.
//
// Keys are split into two namespaces with different futures. Device prefs
// belong to this machine and never leave it. Project pointers are
// server-owned from birth: the desktop impl stores them locally today, but
// the hosted model moves them to per-account state, so callers must not
// assume they are local.

export interface AppStore {
  get(key: string): string | null;
  set(key: string, value: string): void;
  remove(key: string): void;
}

/** Prefs scoped to this device. */
export const DEVICE_PREF_KEYS = {
  appearance: 'maleficium.appearance.v1',
  layout: 'maleficium.layout',
  previewZoom: 'maleficium.previewZoom.v1',
} as const;

/** Pointers into project state. Server-owned from birth. */
export const PROJECT_POINTER_KEYS = {
  recentProjects: 'maleficium.recentProjects.v1',
  mainFileAssoc: 'maleficium.mainFile.v1',
} as const;

let impl: AppStore | null = null;

/** Register the platform implementation. Called once at boot. */
export function setAppStore(next: AppStore) {
  impl = next;
}

export function store(): AppStore {
  if (!impl) throw new Error('app store not configured');
  return impl;
}
