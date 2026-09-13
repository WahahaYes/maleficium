// watcher.ts — live-tracking helpers: debounce + coalesce.
//
// Growth cap: coalesced latest-per-path only; a `git checkout` burst must not
// trigger tree rebuilds, recompiles, or full re-reads per event.
// Wiring to `watch()` from `@tauri-apps/plugin-fs` lives in App integration.

export type WatchKind = 'create' | 'modify' | 'delete';

export interface WatchEventLike {
  kind: WatchKind;
  path: string;
}

/** Collapse a burst to the latest event per path (order: first-seen path). */
export function coalesceEvents(events: WatchEventLike[]): WatchEventLike[] {
  const latest = new Map<string, WatchEventLike>();
  for (const e of events) latest.set(e.path, e);
  return [...latest.values()];
}

/** Map a Tauri plugin-fs WatchEvent to our kinds; returns null for access/other noise. */
export function classifyTauriEvent(ev: { type: unknown; paths: string[] }): WatchEventLike[] {
  const t = ev.type as unknown;
  let kind: WatchKind | null = null;
  if (t === 'any' || t === 'other') kind = 'modify';
  else if (typeof t === 'object' && t !== null) {
    const o = t as Record<string, unknown>;
    if ('create' in o) kind = 'create';
    else if ('remove' in o) kind = 'delete';
    else if ('modify' in o) kind = 'modify';
  }
  if (!kind) return [];
  return ev.paths.map((path) => ({ kind: kind as WatchKind, path }));
}

export function debounce<T extends (...args: never[]) => void>(fn: T, ms = 250): T {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const wrapped = (...args: never[]) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      fn(...args);
    }, ms);
  };
  return wrapped as T;
}
