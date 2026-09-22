import { invoke } from '@tauri-apps/api/core';

/**
 * Runtime project-scope grant.
 *
 * Asks the backend to validate `root` (absolute, resolvable, a directory)
 * mint a recursive fs-scope grant for it, and register it as a session root.
 * Returns the backend's canonical path and the root id it minted.
 *
 * Failure contract: `{ok:false}` with a message, never a throw.
 */
export type GrantResult = {
  ok: boolean;
  path: string | null;
  rootId: string | null;
  error: string | null;
};

export async function grantProjectAccess(root: string): Promise<GrantResult> {
  try {
    const g = await invoke<{ path: string; rootId: string }>('grant_project_access', { root });
    return { ok: true, path: g.path, rootId: g.rootId, error: null };
  } catch (e) {
    return { ok: false, path: null, rootId: null, error: String(e) };
  }
}
