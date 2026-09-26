// mainFile.tauri.ts — Tauri-backed IO for mainFile resolution.
//
// Metadata/paths only; file contents read transiently, never stored.

import { fs } from './fs-provider';
import { resolveMainFile, type MainFileResolution } from './mainFile';
import { getMainFileFor, setMainFileFor } from './mainFile.store';
import { joinPath } from './paths';

async function listTexFilesRecursive(root: string): Promise<string[]> {
  const out: string[] = [];
  const walk = async (dir: string) => {
    let entries;
    try {
      entries = await fs().listDir(dir);
    } catch {
      // Unreadable folder: the scan covers the rest of the project.
      return;
    }
    for (const e of entries) {
      const full = joinPath(dir, e.name);
      if (e.isDirectory) {
        if (e.name === '.git' || e.name === 'out' || e.name === '.maleficium-trash') continue;
        await walk(full);
      } else if (e.isFile && e.name.endsWith('.tex')) {
        out.push(full);
      }
    }
  };
  await walk(root);
  return out;
}

/**
 * Resolve the main file for `root`. `rootId` is the project's grant id
 * (`ProjectGrant.rootId`), the key the explicit association is stored under.
 */
export async function resolveMainFileTauri(
  root: string,
  rootId: string,
  openedFile: string | null,
): Promise<MainFileResolution> {
  return resolveMainFile({
    root,
    openedFile,
    readText: (p) => fs().readText(p),
    listTexFiles: listTexFilesRecursive,
    // App-local store read.
    readConfig: async () => {
      const rel = getMainFileFor(rootId);
      return rel ? JSON.stringify({ mainFile: rel }) : null;
    },
  });
}

/**
 * Persist explicit user association to the app-local store, keyed by the
 * grant's `rootId`. Takes the project root + the file's path (absolute or
 * rel); stores rel.
 */
export async function setMainFile(
  rootId: string,
  root: string,
  absOrRelPath: string,
): Promise<void> {
  const rel = absOrRelPath.startsWith(root + '/')
    ? absOrRelPath.slice(root.length + 1)
    : absOrRelPath;
  setMainFileFor(rootId, rel);
}
