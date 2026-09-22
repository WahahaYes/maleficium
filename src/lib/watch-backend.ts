// watch-backend.ts — the live-tracking seam: changes under a project root.
//
// Desktop watches the filesystem; a hosted client would receive the same
// changes as server events. Consumers see only classified changes, never a
// platform event shape.

import type { WatchChange } from './generated/events';

/** One change to one path under the watched root. */
export interface WatchChangeEvent {
  kind: WatchChange;
  path: string;
}

export interface WatchBackend {
  /** Report batches of changes under `root`. Resolves to the stop function. */
  watch(root: string, cb: (changes: WatchChangeEvent[]) => void): Promise<() => void>;
}

let impl: WatchBackend | null = null;

/** Register the platform implementation. Called once at boot. */
export function setWatchBackend(next: WatchBackend) {
  impl = next;
}

export function watchBackend(): WatchBackend {
  if (!impl) throw new Error('watch backend not configured');
  return impl;
}
