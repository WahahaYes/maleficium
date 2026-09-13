// mainFile.tauri.ts — Tauri-backed IO for mainFile resolution + persistence.
//
// Growth cap: metadata/paths only; file contents read transiently, never stored.

import { readDir, readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { resolveMainFile, type MainFileResolution } from './mainFile';

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
      const full = dir.endsWith('/') ? dir + e.name : dir + '/' + e.name;
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

export async function resolveMainFileTauri(root: string, openedFile: string | null): Promise<MainFileResolution> {
  return resolveMainFile({
    root,
    openedFile,
    readText: (p) => readTextFile(p),
    listTexFiles: listTexFilesRecursive,
    readConfig: async (r) => {
      try {
        return await readTextFile((r.endsWith('/') ? r : r + '/') + '.maleficium.json');
      } catch {
        return null;
      }
    },
  });
}

/** Persist explicit user association: `.maleficium.json` `{mainFile: relPath}`. */
export async function setMainFile(root: string, absOrRelPath: string): Promise<void> {
  const rel = absOrRelPath.startsWith(root + '/') ? absOrRelPath.slice(root.length + 1) : absOrRelPath;
  const cfgPath = (root.endsWith('/') ? root : root + '/') + '.maleficium.json';
  await writeTextFile(cfgPath, JSON.stringify({ mainFile: rel }, null, 2) + '\n');
}
