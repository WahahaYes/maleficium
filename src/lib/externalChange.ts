// externalChange.ts — what an open buffer does when its file changes on disk.
//
// Content decides, not the watch event kind: an in-place write and a
// rename-over save look different to the watcher but leave the same bytes.
// Paths with an unresolved conflict are held here so no save path can write
// over the disk change before the user chooses.

import type { BufferState } from './buffers';

/**
 * - `none`: the disk still holds what the buffer last synced.
 * - `sync`: the disk now matches the buffer's text; mark it clean.
 * - `reload`: no unsaved edits; take the disk content.
 * - `conflict`: unsaved edits and a different disk; the user chooses.
 */
export type ExternalAction = 'none' | 'sync' | 'reload' | 'conflict';

export function decideExternal(
  buf: Pick<BufferState, 'value' | 'dirty' | 'disk'>,
  onDisk: string,
): ExternalAction {
  if (onDisk === buf.disk) return 'none';
  if (onDisk === buf.value) return 'sync';
  if (!buf.dirty) return 'reload';
  return 'conflict';
}

const held = new Set<string>();

/** Hold or release writes to `path` while its conflict is open. */
export function holdWrites(path: string, on: boolean): void {
  if (on) held.add(path);
  else held.delete(path);
}

export function writesHeld(path: string): boolean {
  return held.has(path);
}
