// buffers.ts — multi-file edit buffers with per-file dirty tracking.
//
// Growth cap: Map reference held by App; switching files preserves unsaved
// buffers (no prompt — dirty dot + autosave win). Binary/large files never
// enter the map (caller shows placeholder).

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

export function updateBuffer(
  buffers: Map<string, BufferState>,
  path: string,
  value: string,
): Map<string, BufferState> {
  const next = new Map(buffers);
  const prev = next.get(path);
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
