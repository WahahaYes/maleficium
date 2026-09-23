// externalRefresh.ts — when the shown pdf changes on disk without this app
// compiling it (an agent's compile over MCP), reload the preview.
//
// Pure. Fed the app's own compile events and the polled pdf stamp: a stamp
// that moves while nothing here compiles means someone else rewrote it. The
// first stamp after the app's own compile is adopted, never reloaded, so an
// own compile refreshes exactly once (through its own finish).

import type { AppEvent } from './generated/events';

export type OutputStamp = { mtimeMs: number; bytes: number };

export type RefreshState = {
  /** The stamp last accepted as shown. */
  seen: OutputStamp | null;
  compiling: boolean;
  /** The next poll adopts its stamp without reloading. */
  adopt: boolean;
};

export const INITIAL_REFRESH: RefreshState = { seen: null, compiling: false, adopt: true };

export function onAppEvent(s: RefreshState, e: AppEvent): RefreshState {
  if (e.action === 'compile.start') return { ...s, compiling: true };
  if (e.action === 'compile.finish') return { ...s, compiling: false, adopt: true };
  return s;
}

const same = (a: OutputStamp | null, b: OutputStamp | null) =>
  a === b || (a != null && b != null && a.mtimeMs === b.mtimeMs && a.bytes === b.bytes);

/** Fold one polled stamp; `reload` says the preview must reopen the pdf. */
export function onPoll(
  s: RefreshState,
  now: OutputStamp | null,
): { state: RefreshState; reload: boolean } {
  if (s.compiling) return { state: s, reload: false };
  if (s.adopt) return { state: { ...s, seen: now, adopt: false }, reload: false };
  if (now == null || same(s.seen, now)) return { state: s, reload: false };
  return { state: { ...s, seen: now }, reload: true };
}
