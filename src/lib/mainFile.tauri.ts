// mainFile.tauri.ts — desktop main-file resolution over the core operation
// contract. Resolution order, the association store, and `..`
// canonicalization live in core; this layer only shapes paths.

import { request } from './core-request.tauri';
import type { MainSource } from './generated/api';
import { relUnder } from './paths';

export type MainFileSource = MainSource;

export interface MainFileResolution {
  mainFile: string | null;
  source: MainFileSource;
  /** All tied candidates when the scan found >1; empty otherwise. */
  candidates: string[];
}

/**
 * Resolve the main file for `root`. `rootId` is the project's grant id
 * (`ProjectGrant.rootId`), the key the explicit association is stored under.
 */
export async function resolveMainFileTauri(
  rootId: string,
  openedFile: string | null,
): Promise<MainFileResolution> {
  const r = await request('mainResolve', { rootId, openedAbs: openedFile });
  return { mainFile: r.main, source: r.source, candidates: r.candidates };
}

/**
 * Persist explicit user association to the core store, keyed by the grant's
 * `rootId`. Takes the project root + the file's path (absolute or rel);
 * stores rel.
 */
export async function setMainFile(
  rootId: string,
  root: string,
  absOrRelPath: string,
): Promise<void> {
  const rel = relUnder(root, absOrRelPath) ?? absOrRelPath;
  await request('mainSetAssociation', { rootId, rel }).then(() => undefined);
}
