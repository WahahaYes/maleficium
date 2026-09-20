// mainFile.store.ts — app-local main-file association.
//
// The explicit user association lives only here (`localStorage` map:
// project root → rel path). No in-project file is read or written.

const KEY = 'maleficium.mainFile.v1';

function readAll(): Record<string, string> {
  try {
    const raw = localStorage.getItem(KEY);
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

/** Explicit association for one project root (rel path), or null. */
export function getMainFileFor(root: string): string | null {
  return readAll()[root] ?? null;
}

/** Persist explicit association for one project root. */
export function setMainFileFor(root: string, relPath: string): void {
  const all = readAll();
  all[root] = relPath;
  try {
    localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    /* private mode — association just won't persist */
  }
}
