// fs-provider.tauri.ts — desktop implementation of the filesystem seam.
//
// Project files go to the core file service (one confinement implementation
// for every adapter); only paths outside any granted root (app data, temp,
// dialog destinations) reach plugin-fs.

import { homeDir } from '@tauri-apps/api/path';
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
import {
  mkdir,
  readDir,
  readFile,
  readTextFile,
  remove,
  rename,
  stat,
  writeFile,
  writeTextFile,
} from '@tauri-apps/plugin-fs';
import { lookupProjectRoot } from './fs-provider';
import type { DialogProvider, DirEntry, FileStat, FsProvider } from './fs-provider';
import { request } from './core-request.tauri';
import { dialogStart } from './paths';

function fromCoreStat(s: { size: number; isFile: boolean; isDir: boolean }): FileStat {
  return { size: s.size, isDirectory: s.isDir, isFile: s.isFile };
}

/** The OS home directory, for dialog starting points outside any project. */
export function homeDirectory(): Promise<string> {
  return homeDir();
}

export const desktopFs: FsProvider = {
  readText: async (path) => {
    const p = lookupProjectRoot(path);
    return p ? request('fileRead', { rootId: p.rootId, rel: p.rel }) : readTextFile(path);
  },
  writeText: async (path, contents) => {
    const p = lookupProjectRoot(path);
    if (p) await request('fileWrite', { rootId: p.rootId, rel: p.rel, text: contents });
    else await writeTextFile(path, contents);
  },
  readBytes: async (path) => {
    const p = lookupProjectRoot(path);
    if (!p) return readFile(path);
    const b64 = await request('fileReadBytes', { rootId: p.rootId, rel: p.rel });
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  },
  writeBytes: async (path, contents, opts) => {
    const p = lookupProjectRoot(path);
    if (p) {
      if (opts?.append) throw new Error('append is not a project operation: ' + path);
      await request('fileWrite', {
        rootId: p.rootId,
        rel: p.rel,
        text: new TextDecoder('utf-8', { fatal: true }).decode(contents),
      });
    } else if (opts) await writeFile(path, contents, opts);
    else await writeFile(path, contents);
  },
  listDir: async (path): Promise<DirEntry[]> => {
    const p = lookupProjectRoot(path);
    if (!p) {
      const entries = await readDir(path);
      return entries.map((e) => ({
        name: e.name,
        isDirectory: e.isDirectory,
        isFile: e.isFile,
      }));
    }
    const entries = await request('fileList', { rootId: p.rootId, rel: p.rel });
    return entries.map((e) => ({
      name: e.name,
      isDirectory: e.entryType === 'dir',
      isFile: e.entryType === 'file',
    }));
  },
  rename: async (from, to) => {
    const f = lookupProjectRoot(from);
    const t = lookupProjectRoot(to);
    if (f || t) {
      if (!f || !t || f.rootId !== t.rootId)
        throw new Error('rename across the project boundary: ' + from);
      await request('fileRename', { rootId: f.rootId, oldRel: f.rel, newRel: t.rel });
    } else await rename(from, to);
  },
  remove: async (path, opts) => {
    const p = lookupProjectRoot(path);
    if (p)
      await request('fileRemove', {
        rootId: p.rootId,
        rel: p.rel,
        recursive: opts?.recursive ?? false,
      });
    else if (opts) await remove(path, opts);
    else await remove(path);
  },
  mkdir: async (path, opts) => {
    const p = lookupProjectRoot(path);
    if (p) await request('fileMkdir', { rootId: p.rootId, rel: p.rel });
    else if (opts) await mkdir(path, opts);
    else await mkdir(path);
  },
  stat: async (path): Promise<FileStat | null> => {
    const p = lookupProjectRoot(path);
    if (!p) {
      try {
        const info = await stat(path);
        return { size: info.size, isDirectory: info.isDirectory, isFile: info.isFile };
      } catch {
        // The seam's contract: a path that cannot be stat'ed reads as absent.
        return null;
      }
    }
    const s = await request('fileStat', { rootId: p.rootId, rel: p.rel });
    return s ? fromCoreStat(s) : null;
  },
};

export const desktopDialog: DialogProvider = {
  openDirectory: async (opts) => {
    const defaultPath = dialogStart(opts?.defaultPath, await homeDir());
    const picked = await openDialog({ directory: true, multiple: false, ...opts, defaultPath });
    return typeof picked === 'string' ? picked : null;
  },
  saveFile: async (opts) => {
    const defaultPath = dialogStart(opts?.defaultPath, await homeDir());
    const picked = await saveDialog({ ...opts, defaultPath });
    return typeof picked === 'string' ? picked : null;
  },
};
