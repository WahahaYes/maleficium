// buffers.ts — multi-file edit buffers with per-file dirty tracking.
//
// Switching files preserves unsaved buffers (dirty dot, no prompt).
// Binary/large files never enter the map.

export interface BufferState {
  value: string;
  dirty: boolean;
  /** Monotonic version for memo identity per buffer. */
  version: number;
}

export function getOrCreateBuffer(
  buffers: Map<string, BufferState>,
  path: string,
  loadedValue: string,
): BufferState {
  const existing = buffers.get(path);
  if (existing) return existing;
  const next: BufferState = { value: loadedValue, dirty: false, version: 0 };
  buffers.set(path, next);
  return next;
}

/**
 * Record an edit. A value equal to what the buffer already holds is an echo,
 * not an edit: the map is returned untouched so nothing is marked dirty.
 */
export function updateBuffer(
  buffers: Map<string, BufferState>,
  path: string,
  value: string,
): Map<string, BufferState> {
  const prev = buffers.get(path);
  if (prev && prev.value === value) return buffers;
  const next = new Map(buffers);
  next.set(path, { value, dirty: true, version: (prev?.version ?? 0) + 1 });
  return next;
}

export function markSaved(
  buffers: Map<string, BufferState>,
  path: string,
): Map<string, BufferState> {
  const prev = buffers.get(path);
  if (!prev || !prev.dirty) return buffers;
  const next = new Map(buffers);
  next.set(path, { ...prev, dirty: false });
  return next;
}

export const MAX_BUFFERS = 10;

export function enforceBufferCap(
  buffers: Map<string, BufferState>,
  active: string,
): Map<string, BufferState> {
  if (buffers.size <= MAX_BUFFERS) return buffers;
  const next = new Map(buffers);
  for (const k of [...next.keys()]) {
    if (next.size <= MAX_BUFFERS) break;
    if (k === active) continue;
    const b = next.get(k);
    if (b && !b.dirty) next.delete(k);
  }
  return next;
}
