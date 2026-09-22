// recentProjects.ts — on-device store of opened projects + recents.
//
// Remembers opened project roots (most-recent-first) so launch restores the
// last project instead of an empty shell. Storage is app-local only: the
// project dir is never touched. Entries are validated on read (absolute,
// trimmed, deduped, capped); stale roots are pruned at restore time.

import { PROJECT_POINTER_KEYS, store } from './app-store';

const KEY = PROJECT_POINTER_KEYS.recentProjects;

/** Cap: recents are a short jump-list, not a log. */
export const MAX_RECENT_PROJECTS = 10;

function clean(list: unknown): string[] {
  if (!Array.isArray(list)) return [];
  const seen = new Set<string>();
  const out: string[] = [];
  for (const item of list) {
    if (typeof item !== 'string') continue;
    const p = item.trim();
    if (!p || !p.startsWith('/') || seen.has(p)) continue;
    seen.add(p);
    out.push(p);
  }
  return out.slice(0, MAX_RECENT_PROJECTS);
}

/** Most-recent-first project roots (possibly stale). */
export function getRecentProjects(): string[] {
  try {
    const raw = store().get(KEY);
    if (!raw) return [];
    return clean(JSON.parse(raw));
  } catch {
    // Unparseable stored recents: start with none.
    return [];
  }
}

/** Record an open: moves `root` to the front, dedupes, caps. */
export function touchRecentProject(root: string): string[] {
  const p = root.trim();
  const next = clean([p, ...getRecentProjects()]);
  store().set(KEY, JSON.stringify(next));
  return next;
}

/** Drop roots that no longer resolve. */
export function pruneRecentProjects(keep: (root: string) => boolean): string[] {
  const next = getRecentProjects().filter(keep);
  store().set(KEY, JSON.stringify(next));
  return next;
}
