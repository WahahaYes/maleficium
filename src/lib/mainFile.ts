// mainFile.ts — resolved LaTeX root document for a project.
//
// Scalar path reference only, never file payload. Resolution order:
// explicit association → `%!TEX root` magic → `\documentclass` scan
// (first wins, deterministic) → single-.tex fallback → none. Never throws.

import { dirName, isAbsolutePath, joinPath } from './paths';

export type MainFileSource = 'config' | 'magic' | 'scan' | 'single' | 'none';

export interface MainFileResolution {
  mainFile: string | null;
  source: MainFileSource;
  /** All tied candidates (sorted) when scan found >1; empty otherwise. */
  candidates: string[];
}

export interface MainFileDeps {
  root: string;
  /** Absolute path of the currently opened file (may be non-main). */
  openedFile: string | null;
  /** Read a text file (injected). */
  readText: (absPath: string) => Promise<string>;
  /** List absolute .tex paths under root (injected). */
  listTexFiles: (root: string) => Promise<string[]>;
  /** Read raw association JSON from the app-local store (injected). */
  readConfig: (root: string) => Promise<string | null>;
}

const MAGIC_RE = /^\s*%\s*!\s*TEX\s+root\s*=\s*(.+?)\s*$/im;
const DOCUMENTCLASS_RE = /\\documentclass(\[[^\]]*\])?\{[^}]*\}/;

export function parseMagicComment(content: string): string | null {
  const m = content.match(MAGIC_RE);
  if (!m) return null;
  const v = m[1]
    .trim()
    .replace(/^["']|["']$/g, '')
    .trim();
  return v || null;
}

export function hasDocumentclass(content: string): boolean {
  return DOCUMENTCLASS_RE.test(content);
}

function parseConfigMain(raw: string | null, root: string): string | null {
  if (!raw) return null;
  try {
    const j = JSON.parse(raw) as { mainFile?: unknown };
    if (typeof j.mainFile === 'string' && j.mainFile.trim()) {
      const v = j.mainFile.trim();
      return isAbsolutePath(v) ? v : joinPath(root, v);
    }
  } catch {
    /* unparseable stored association — fall through to detection */
  }
  return null;
}

/** Resolve with injectable IO. */
export async function resolveMainFile(deps: MainFileDeps): Promise<MainFileResolution> {
  const { root, openedFile, readText, listTexFiles, readConfig } = deps;
  try {
    // 1) explicit association
    const cfg = await readConfig(root).catch(() => null);
    const fromCfg = parseConfigMain(cfg, root);
    if (fromCfg) return { mainFile: fromCfg, source: 'config', candidates: [] };

    // 2) magic comment in the opened file
    if (openedFile) {
      try {
        const content = await readText(openedFile);
        const magic = parseMagicComment(content);
        if (magic) {
          const base = dirName(openedFile);
          const resolved = isAbsolutePath(magic) ? magic : joinPath(base || root, magic);
          return { mainFile: resolved, source: 'magic', candidates: [] };
        }
      } catch {
        /* unreadable opened file — fall through to scan */
      }
    }

    // 3) scan for \documentclass (deterministic: sorted, first wins)
    let texFiles: string[] = [];
    try {
      texFiles = (await listTexFiles(root)).slice().sort();
    } catch {
      // Project unlistable: only the magic comment could have named a main file.
      texFiles = [];
    }
    const withClass: string[] = [];
    for (const f of texFiles) {
      try {
        const c = await readText(f);
        if (hasDocumentclass(c)) withClass.push(f);
      } catch {
        /* skip unreadable files */
      }
    }
    if (withClass.length >= 1) {
      return {
        mainFile: withClass[0],
        source: 'scan',
        candidates: withClass.length > 1 ? withClass : [],
      };
    }

    // 4) single-.tex fallback
    if (texFiles.length === 1) {
      return { mainFile: texFiles[0], source: 'single', candidates: [] };
    }
    return { mainFile: null, source: 'none', candidates: [] };
  } catch {
    // Resolution never throws: an unexpected IO failure reads as "no main file".
    return { mainFile: null, source: 'none', candidates: [] };
  }
}
