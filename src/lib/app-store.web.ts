// app-store.web.ts — localStorage implementation of the key/value seam.
//
// Swallows storage errors: private mode yields null reads and dropped writes
// rather than throwing into callers.

import type { AppStore } from './app-store';

export const localAppStore: AppStore = {
  get(key) {
    try {
      return localStorage.getItem(key);
    } catch {
      /* storage unavailable (private mode) — read as unset */
      return null;
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(key, value);
    } catch {
      /* storage unavailable — value just will not persist */
    }
  },
  remove(key) {
    try {
      localStorage.removeItem(key);
    } catch {
      /* storage unavailable */
    }
  },
};
