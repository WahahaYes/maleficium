// mainFile.tauri.ts — Tauri-backed IO for mainFile resolution.
//
// Growth cap: metadata/paths only; file contents read transiently, never stored.

import { readDir, readTextFile } from '@tauri-apps/plugin-fs';
import { resolveMainFile, type MainFileResolution } from './mainFile';
import { getMainFileFor } from './mainFile.store';
import { joinPath } from './paths';

async function listTexFilesRecursive(root: string): Promise<string[]> {
  const out: string[] = [];
  const walk = async (dir: string) => {
    let entries;
    try {
      entries = await readDir(dir);
    } catch {
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

export async function resolveMainFileTauri(
  root: string,
  openedFile: string | null,
): Promise<MainFileResolution> {
  return resolveMainFile({
    root,
    openedFile,
    readText: (p) => readTextFile(p),
    listTexFiles: listTexFilesRecursive,
    // App-local store (V-3 clean cut — no in-project file read).
    readConfig: async (r) => {
      const rel = getMainFileFor(r);
      return rel ? JSON.stringify({ mainFile: rel }) : null;
    },
  });
}

/**
 * Persist explicit user association to the app-local store.
 * Takes the project root + the file's path (absolute or rel); stores rel.
 */
export async function setMainFile(root: string, absOrRelPath: string): Promise<void> {
  const { setMainFileFor } = await import('./mainFile.store');
  const rel = absOrRelPath.startsWith(root + '/')
    ? absOrRelPath.slice(root.length + 1)
    : absOrRelPath;
  setMainFileFor(root, rel);
}
