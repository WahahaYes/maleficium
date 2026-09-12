import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog'
import { readTextFile, writeTextFile, readDir } from '@tauri-apps/plugin-fs'

export type TreeEntry = { name: string; path: string; type: 'dir' | 'file'; children?: TreeEntry[] }

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
    return [...dirs, ...result]
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