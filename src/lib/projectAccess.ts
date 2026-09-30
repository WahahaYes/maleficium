import { bindProjectRoot } from './fs-provider';
import { request } from './core-request.tauri';

/**
 * Runtime project grant.
 *
 * Asks the backend to validate `root` (absolute, resolvable, a directory)
 * and register it as a session root, then binds it for the desktop file
 * seam so project paths resolve to the core file service. The recursive
 * fs-scope grant beside it stays until the core watcher lands: it is the
 * watcher's only path to project dirs. File IO no longer needs it.
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
    const g = await request('grantProjectAccess', { root });
    bindProjectRoot(g.rootId, g.path);
    return { ok: true, path: g.path, rootId: g.rootId, error: null };
  } catch (e) {
    return { ok: false, path: null, rootId: null, error: String(e) };
  }
}

/** The backend-owned scratch root untitled documents compile in. */
export async function grantUntitledAccess(): Promise<GrantResult> {
  try {
    const g = await request('grantUntitledAccess', {});
    bindProjectRoot(g.rootId, g.path);
    return { ok: true, path: g.path, rootId: g.rootId, error: null };
  } catch (e) {
    return { ok: false, path: null, rootId: null, error: String(e) };
  }
}
