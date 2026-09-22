// watch-backend.tauri.ts — desktop implementation of the watch seam: the
// plugin-fs recursive watcher, its events classified into changes.

import { watch, type WatchEvent, type WatchEventKind } from '@tauri-apps/plugin-fs';
import type { WatchChange } from './generated/events';
import type { WatchBackend, WatchChangeEvent } from './watch-backend';

/** The change a plugin-fs event kind means; null for access noise. */
export function changeOf(type: WatchEventKind): WatchChange | null {
  if (type === 'any' || type === 'other') return 'modify';
  if ('create' in type) return 'create';
  if ('remove' in type) return 'delete';
  if ('modify' in type) return 'modify';
  return null;
}

/** One plugin-fs event as changes, one per path it names. */
export function classify(ev: Pick<WatchEvent, 'type' | 'paths'>): WatchChangeEvent[] {
  const kind = changeOf(ev.type);
  return kind ? ev.paths.map((path) => ({ kind, path })) : [];
}

export const desktopWatch: WatchBackend = {
  watch: (root, cb) =>
    watch(
      root,
      (ev) => {
        const changes = classify(ev);
        if (changes.length > 0) cb(changes);
      },
      { recursive: true, delayMs: 250 },
    ),
};
