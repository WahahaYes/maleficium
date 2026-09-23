// useIndexOverlays.ts — keep the project index in line with unsaved buffers.
//
// Each dirty buffer inside the project is laid over its file in the index
// (debounced while typing); a buffer that is saved, reverted or closed lifts
// its overlay. A new project starts from none: opening the index drops them.

import { useEffect, useRef } from 'react';
import { emit } from '../lib/events';
import { overlayDelta, projectIndex } from '../lib/project-index';
import type { BufferState } from '../lib/buffers';

const OVERLAY_DEBOUNCE_MS = 300;

export function useIndexOverlays(
  projectId: string | null,
  buffers: ReadonlyMap<string, BufferState>,
  relInProject: (abs: string) => string | null,
) {
  const sent = useRef(new Map<string, string>());
  useEffect(() => {
    sent.current = new Map();
  }, [projectId]);

  useEffect(() => {
    if (!projectId) return;
    const t = setTimeout(() => {
      const dirty = new Map<string, string>();
      for (const [path, b] of buffers) {
        const rel = relInProject(path);
        if (rel && b.dirty) dirty.set(rel, b.value);
      }
      for (const [rel, text] of overlayDelta(sent.current, dirty)) {
        if (text === null) sent.current.delete(rel);
        else sent.current.set(rel, text);
        projectIndex()
          .overlay(projectId, rel, text)
          .catch((e: unknown) =>
            emit({
              scope: 'fs',
              kind: 'warn',
              actor: 'system',
              message: 'project index not updated: ' + String(e).slice(0, 120),
              event: { action: 'index.failed', root: projectId, error: String(e).slice(0, 200) },
            }),
          );
      }
    }, OVERLAY_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [projectId, buffers, relInProject]);
}
