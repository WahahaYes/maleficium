// watcher.ts — live-tracking helpers: debounce + coalesce.
//
// Coalesced latest-per-path only; bursts never trigger per-event rebuilds,
// recompiles, or full re-reads.

import type { WatchChangeEvent } from './watch-backend';

/** Collapse a burst to the latest event per path (order: first-seen path). */
export function coalesceEvents(events: WatchChangeEvent[]): WatchChangeEvent[] {
  const latest = new Map<string, WatchChangeEvent>();
  for (const e of events) latest.set(e.path, e);
  return [...latest.values()];
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
