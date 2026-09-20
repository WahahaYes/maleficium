// mainFile.store.ts — app-local main-file association.
//
// The explicit user association lives only here (app-store map: projectId →
// rel path). Keyed by project id, not by absolute root, so the association
// survives the move to server-owned project state. No in-project file is
// read or written.

import { PROJECT_POINTER_KEYS, store } from './app-store';

const KEY = PROJECT_POINTER_KEYS.mainFileAssoc;

function readAll(): Record<string, string> {
  try {
    const raw = store().get(KEY);
    if (!raw) return {};
    const j = JSON.parse(raw) as unknown;
    if (typeof j !== 'object' || j === null) return {};
    const out: Record<string, string> = {};
    for (const [k, v] of Object.entries(j as Record<string, unknown>)) {
      if (typeof v === 'string' && v.trim()) out[k] = v;
    }
    return out;
  } catch {
    return {};
  }
}

/** Explicit association for one project (rel path), or null. */
export function getMainFileFor(projectId: string): string | null {
  return readAll()[projectId] ?? null;
}

/** Persist explicit association for one project. */
export function setMainFileFor(projectId: string, relPath: string): void {
  const all = readAll();
  all[projectId] = relPath;
  store().set(KEY, JSON.stringify(all));
}
