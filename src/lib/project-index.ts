// project-index.ts — the project index seam: every file of the open project,
// its text, and its symbols, held by the Rust core.
//
// The app builds it on open, reports watcher batches, and lays unsaved
// buffers over it, so search and navigation read what the user sees.

import type {
  FileMatch,
  Query,
  ReplaceApplied,
  ReplacePreview,
  SearchResult,
} from './generated/index';

export type { FileMatch, Query, ReplaceApplied, ReplacePreview, SearchResult };

export interface ProjectIndexProvider {
  /** Build the index now (dropping overlays); resolves to the files listed. */
  open(rootId: string): Promise<number>;
  /** A watcher now reports this root's changes (or stopped). */
  watched(rootId: string, watched: boolean): Promise<void>;
  /** One watcher batch of absolute paths. */
  touch(rootId: string, paths: string[]): Promise<void>;
  /** Lay unsaved text over a root-relative file; null lifts it. */
  overlay(rootId: string, rel: string, text: string | null): Promise<void>;
  /** Search every text file; files of `mainRel`'s document rank first. */
  search(rootId: string, query: Query, mainRel: string | null): Promise<SearchResult>;
  /** Files matching a fuzzy name query, best first. */
  findFiles(rootId: string, query: string): Promise<FileMatch[]>;
  /** Plan replacing every match; writes nothing. */
  replacePreview(
    rootId: string,
    query: Query,
    replacement: string,
    mainRel: string | null,
  ): Promise<ReplacePreview>;
  /**
   * Apply a plan by token. Files in `keepOpen` (open buffers) are not
   * written; their new text comes back in `edits`.
   */
  replaceApply(rootId: string, token: string, keepOpen: string[]): Promise<ReplaceApplied>;
}

let impl: ProjectIndexProvider | null = null;

/** Register the platform implementation. Called once at boot. */
export function setProjectIndex(next: ProjectIndexProvider) {
  impl = next;
}

export function projectIndex(): ProjectIndexProvider {
  if (!impl) throw new Error('project index not configured');
  return impl;
}

/**
 * The overlay changes that bring the index in line with the buffers: text
 * for each dirty buffer whose value differs from what was sent, null for
 * each sent overlay whose buffer is clean or gone. Pure.
 */
export function overlayDelta(
  sent: ReadonlyMap<string, string>,
  dirty: ReadonlyMap<string, string>,
): Array<[string, string | null]> {
  const out: Array<[string, string | null]> = [];
  for (const [rel, text] of dirty) if (sent.get(rel) !== text) out.push([rel, text]);
  for (const rel of sent.keys()) if (!dirty.has(rel)) out.push([rel, null]);
  return out;
}
