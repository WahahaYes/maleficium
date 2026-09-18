import { invoke } from '@tauri-apps/api/core';

/**
 * Runtime project-scope grant (trust-boundary Slice A).
 *
 * Asks the backend to validate `root` (absolute, resolvable, a directory)
 * and mint a recursive fs-scope grant for it. Returns the backend's
 * canonical path on success — callers adopt it as `root` so later
 * comparisons are canonical-vs-canonical.
 *
 * Failure contract (matches `compile.ts`/`synctex.ts` style):
 * `{ok:false}` with a message, never a throw.
 */
export type GrantResult = { ok: boolean; path: string | null; error: string | null };

export async function grantProjectAccess(root: string): Promise<GrantResult> {
  try {
    const path = await invoke<string>('grant_project_access', { root });
    return { ok: true, path, error: null };
  } catch (e) {
    return { ok: false, path: null, error: String(e) };
  }
}
