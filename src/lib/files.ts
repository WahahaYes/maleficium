import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog'
import { readTextFile, writeTextFile, readDir } from '@tauri-apps/plugin-fs'

export type TreeEntry = { name: string; path: string; type: 'dir' | 'file'; children?: TreeEntry[] }

/** Files/dirs never shown when `filterHidden` is on (build artifacts + trash). */
const HIDDEN_EXACT = new Set(['.git', '.maleficium-trash', 'out']);
const HIDDEN_SUFFIX = ['.aux', '.log', '.fls', '.fdb_latexmk', '.synctex.gz'];

/** Bytes above which the editor shows an honest placeholder instead of full text. */
export const LARGE_FILE_BYTES = 500_000;

export function isHiddenName(name: string): boolean {
  if (HIDDEN_EXACT.has(name)) return true;
  if (name.startsWith('.') && name !== '.maleficium.json' && name !== '.gitignore') return true;
  return HIDDEN_SUFFIX.some((s) => name.endsWith(s));
}

/** Sort dirs-first, case-insensitive (deterministic: tie-break by raw name). */
export function sortTreeEntries<T extends { name: string; type: string }>(entries: T[]): T[] {
  return [...entries].sort((a, b) => {
    const aDir = a.type === 'dir' ? 0 : 1;
    const bDir = b.type === 'dir' ? 0 : 1;
    if (aDir !== bDir) return aDir - bDir;
    const c = a.name.toLowerCase().localeCompare(b.name.toLowerCase());
    return c !== 0 ? c : (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
  });
}

export async function openProject(): Promise<string | null> {
  const path = await openDialog({ directory: true })
  return path ?? null
}

export async function listTree(root: string): Promise<TreeEntry[]> {
  try {
    const entries = await readDir(root)
    const result: TreeEntry[] = []
    const dirs: TreeEntry[] = []
    for (const entry of entries) {
      if (isHiddenName(entry.name)) continue;
      const isDir = 'children' in entry ? !!entry.children : entry.isDirectory
      const fullPath = root.endsWith('/') ? root + entry.name : root + '/' + entry.name
      const child: TreeEntry = { name: entry.name, path: fullPath, type: isDir ? 'dir' : 'file' }
      if (isDir) {
        const sub = await listTree(fullPath)
        child.children = sub
        dirs.push(child)
      } else {
        result.push(child)
      }
    }
    return [...sortTreeEntries(dirs), ...sortTreeEntries(result)]
  } catch {
    return []
  }
}

/** Single-level listing for lazy tree expansion (metadata only, sorted). */
export async function listDir1Level(dir: string): Promise<TreeEntry[]> {
  try {
    const entries = await readDir(dir)
    const out: TreeEntry[] = []
    for (const entry of entries) {
      if (isHiddenName(entry.name)) continue;
      const isDir = 'children' in entry ? !!entry.children : entry.isDirectory
      const fullPath = dir.endsWith('/') ? dir + entry.name : dir + '/' + entry.name
      out.push({ name: entry.name, path: fullPath, type: isDir ? 'dir' : 'file' });
    }
    return sortTreeEntries(out);
  } catch {
    return []
  }
}

export async function loadTex(path: string): Promise<string> {
  return await readTextFile(path)
}

export async function saveTex(path: string, content: string): Promise<void> {
  await writeTextFile(path, content)
}

export async function loadTexViaDialog(): Promise<{ name: string; content: string } | null> {
  try {
    const path = await openDialog({ filters: [{ name: 'LaTeX', extensions: ['tex'] }] })
    if (!path) return null
    const content = await readTextFile(path)
    return { name: path, content }
  } catch {
    return null
  }
}

export async function saveTexToDisk(name: string, content: string): Promise<void> {
  try {
    const path = await saveDialog({ defaultPath: name, filters: [{ name: 'LaTeX', extensions: ['tex'] }] })
    if (path) await writeTextFile(path, content)
  } catch {
    const blob = new Blob([content], { type: 'text/plain' })
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = name
    anchor.click()
    URL.revokeObjectURL(url)
  }
}

export function helloName(): string {
  return 'Hello!'
}