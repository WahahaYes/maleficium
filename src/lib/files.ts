import { dialog, fs } from './fs-provider';
import { joinPath } from './paths';

export type TreeEntry = {
  name: string;
  path: string;
  type: 'dir' | 'file';
  children?: TreeEntry[];
};

/** Lowercased extensions that get a rich preview instead of the text editor. */
const IMAGE_EXT = new Set(['.png', '.jpg', '.jpeg', '.gif', '.bmp', '.webp', '.svg']);
const VIDEO_EXT = new Set(['.mp4', '.webm', '.ogv', '.mov', '.mkv']);
const PDF_EXT = new Set(['.pdf']);

export type PreviewKind = 'text' | 'image' | 'video' | 'pdf' | 'binary';

/** Extensions that open as editable text (everything else previews). */
const TEXT_EXT = new Set([
  '.tex',
  '.bib',
  '.sty',
  '.cls',
  '.md',
  '.markdown',
  '.txt',
  '.log',
  '.aux',
  '.toc',
  '.lof',
  '.lot',
  '.out',
  '.fls',
  '.json',
  '.yaml',
  '.yml',
  '.toml',
  '.xml',
  '.html',
  '.htm',
  '.css',
  '.js',
  '.ts',
  '.tsx',
  '.jsx',
  '.py',
  '.sh',
  '.csv',
  '.r',
  '.jl',
]);

/** Classify a path for the editor-vs-preview decision (extension only, cheap). */
export function extOf(path: string): string {
  const dot = path.lastIndexOf('.');
  return dot >= 0 ? path.slice(dot).toLowerCase() : '';
}

/** MIME type for object-URL previews. Single ext→mime table. */
const MIME_FOR_EXT: Record<string, string> = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.bmp': 'image/bmp',
  '.webp': 'image/webp',
  '.svg': 'image/svg+xml',
  '.mp4': 'video/mp4',
  '.webm': 'video/webm',
  '.ogv': 'video/ogg',
  '.ogg': 'video/ogg',
  '.mov': 'video/quicktime',
  '.mkv': 'video/x-matroska',
};

/** MIME type for a path; unknown → application/octet-stream. */
export function mimeFor(path: string): string {
  return MIME_FOR_EXT[extOf(path)] ?? 'application/octet-stream';
}

/** Classify a path for the editor-vs-preview decision (extension only, cheap). */
export function previewKindFor(path: string): PreviewKind {
  const ext = extOf(path);
  if (IMAGE_EXT.has(ext)) return 'image';
  if (VIDEO_EXT.has(ext)) return 'video';
  if (PDF_EXT.has(ext)) return 'pdf';
  if (!ext || TEXT_EXT.has(ext)) return 'text';
  return 'binary';
}

/** True when the path should open in a preview surface, not the text editor. */
export function isPreviewable(path: string): boolean {
  return previewKindFor(path) !== 'text';
}

/** Read a file as bytes for object-URL previews (images/video/pdf). */
export async function loadPreviewBytes(path: string): Promise<Uint8Array> {
  return await fs().readBytes(path);
}

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
    return c !== 0 ? c : a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
  });
}

export async function openProject(): Promise<string | null> {
  // Preset hook: `?project=/abs/dir` (or `#project=…`) preselects the root
  // without a dialog. No param → dialog.
  try {
    const q = new URLSearchParams(window.location.search);
    const h = window.location.hash.match(/project=([^&]+)/);
    const preset = q.get('project') ?? (h ? decodeURIComponent(h[1]) : null);
    if (preset) return preset;
  } catch {
    /* fall through to dialog */
  }
  const path = await dialog().openDirectory({ recursive: true });
  return path ?? null;
}

/** Create an empty file (parents must exist); returns the absolute path. */
export async function createFile(dir: string, name: string): Promise<string> {
  const clean = name.trim().replace(/\//g, '_') || 'untitled.tex';
  const full = joinPath(dir, clean);
  await fs().writeBytes(full, new Uint8Array());
  return full;
}

/** Rename within the same tree; returns the new absolute path. */
export async function renamePath(oldPath: string, newName: string): Promise<string> {
  const clean = newName.trim().replace(/\//g, '_');
  if (!clean) throw new Error('empty name');
  const dir = oldPath.slice(0, oldPath.lastIndexOf('/'));
  const full = dir + '/' + clean;
  await fs().rename(oldPath, full);
  return full;
}

/** Single-level listing for lazy tree expansion (metadata only, sorted). */
/** Full recursive walk for background scans — never on the open path. */
export async function listTreeDeep(root: string): Promise<TreeEntry[]> {
  try {
    const entries = await fs().listDir(root);
    const result: TreeEntry[] = [];
    const dirs: TreeEntry[] = [];
    for (const entry of entries) {
      if (isHiddenName(entry.name)) continue;
      const isDir = 'children' in entry ? !!entry.children : entry.isDirectory;
      const fullPath = joinPath(root, entry.name);
      const child: TreeEntry = { name: entry.name, path: fullPath, type: isDir ? 'dir' : 'file' };
      if (isDir) {
        const sub = await listTreeDeep(fullPath);
        child.children = sub;
        dirs.push(child);
      } else {
        result.push(child);
      }
    }
    return [...sortTreeEntries(dirs), ...sortTreeEntries(result)];
  } catch {
    return [];
  }
}

export async function listDir1Level(dir: string): Promise<TreeEntry[]> {
  try {
    const entries = await fs().listDir(dir);
    const out: TreeEntry[] = [];
    for (const entry of entries) {
      if (isHiddenName(entry.name)) continue;
      const isDir = 'children' in entry ? !!entry.children : entry.isDirectory;
      const fullPath = joinPath(dir, entry.name);
      out.push({ name: entry.name, path: fullPath, type: isDir ? 'dir' : 'file' });
    }
    return sortTreeEntries(out);
  } catch {
    return [];
  }
}

export async function loadTex(path: string): Promise<string> {
  return await fs().readText(path);
}

export async function saveTex(path: string, content: string): Promise<void> {
  await fs().writeText(path, content);
}

export async function saveTexToDisk(name: string, content: string): Promise<void> {
  try {
    const path = await dialog().saveFile({
      defaultPath: name,
      filters: [{ name: 'LaTeX', extensions: ['tex'] }],
    });
    if (path) await fs().writeText(path, content);
  } catch {
    const blob = new Blob([content], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = name;
    anchor.click();
    URL.revokeObjectURL(url);
  }
}
