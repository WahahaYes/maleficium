// fs-provider.tauri.ts — desktop implementation of the filesystem seam.
//
// The only module that imports plugin-fs and plugin-dialog for general file
// work. Calls pass through unchanged; `stat` maps a missing path to null.

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
import type { DialogProvider, DirEntry, FileStat, FsProvider } from './fs-provider';
import { dialogStart } from './paths';

export const desktopFs: FsProvider = {
  readText: (path) => readTextFile(path),
  writeText: (path, contents) => writeTextFile(path, contents),
  readBytes: (path) => readFile(path),
  writeBytes: (path, contents, opts) =>
    opts ? writeFile(path, contents, opts) : writeFile(path, contents),
  listDir: async (path): Promise<DirEntry[]> => {
    const entries = await readDir(path);
    return entries.map((e) => ({
      name: e.name,
      isDirectory: e.isDirectory,
      isFile: e.isFile,
    }));
  },
  rename: (from, to) => rename(from, to),
  remove: (path, opts) => (opts ? remove(path, opts) : remove(path)),
  mkdir: (path, opts) => (opts ? mkdir(path, opts) : mkdir(path)),
  stat: async (path): Promise<FileStat | null> => {
    try {
      const info = await stat(path);
      return { size: info.size, isDirectory: info.isDirectory, isFile: info.isFile };
    } catch {
      // The seam's contract: a path that cannot be stat'ed reads as absent.
      return null;
    }
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
