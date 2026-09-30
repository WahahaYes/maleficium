// watch-backend.tauri.ts — desktop implementation of the watch seam: the
// core watcher over the operation contract.
//
// Core owns the watcher thread (it feeds the index and filters our own
// writes there); this backend only drains its event queue on a timer and
// reports root-relative paths as absolute ones. Polling keeps the adapter
// stateless: no listener to leak when effects re-run under StrictMode.

import { lookupProjectRoot } from './fs-provider';
import { joinPath } from './paths';
import { request } from './core-request.tauri';
import type { WatchBackend } from './watch-backend';

/** Drained often enough to feel live, seldom enough to stay cheap. */
export const WATCH_POLL_MS = 500;

export const desktopWatch: WatchBackend = {
  watch: async (root, cb) => {
    const proj = lookupProjectRoot(root);
    if (!proj) throw new Error('outside any open project: ' + root);
    const { rootId } = proj;
    await request('watchStart', { rootId });
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      if (stopped) return;
      try {
        const events = await request('watchPoll', { rootId });
        if (!stopped && events.length > 0) {
          cb(
            events.map((e) => ({
              kind: e.change,
              path: joinPath(root, e.rel),
            })),
          );
        }
      } catch {
        // A failed poll drops its batch; the next tick retries. Stopping
        // still ends the loop, so a dead backend cannot wedge the app.
      }
      if (!stopped) timer = setTimeout(poll, WATCH_POLL_MS);
    };
    timer = setTimeout(poll, WATCH_POLL_MS);
    return () => {
      stopped = true;
      if (timer) clearTimeout(timer);
      request('watchStop', { rootId }).catch(() => {});
    };
  },
};
