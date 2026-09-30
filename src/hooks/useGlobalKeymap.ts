import { useEffect, type MutableRefObject } from 'react';
import type { BufferState } from '../lib/buffers';
import {
  matchesCompile,
  matchesForwardSync,
  matchesGoToDefinition,
  matchesMenuChord,
  menuChordId,
  zoomChord,
} from '../lib/keymap';
import type { ZoomAction } from '../lib/zoom';
import { useLatest } from './useLatest';

export interface KeymapActions {
  compile: () => void;
  goToDefinition: () => void;
  forwardSync: () => void;
  zoom: (a: ZoomAction) => void;
  menuAction: (id: string) => void;
  showShortcuts: () => void;
  toggleTree: () => void;
  select: (path: string) => void;
}

// The window-level keymap. It subscribes once and calls the latest actions,
// so a chord never runs against stale render state. Menu chords dispatch
// through the command registry: one path, no duplicates.
export function useGlobalKeymap(
  actions: KeymapActions,
  buffersRef: MutableRefObject<Map<string, BufferState>>,
  fileNameRef: MutableRefObject<string>,
) {
  const act = useLatest(actions);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      const a = act.current;
      const zoom = zoomChord(e);
      if (matchesCompile(e)) {
        e.preventDefault();
        a.compile();
      } else if (matchesGoToDefinition(e)) {
        e.preventDefault();
        a.goToDefinition();
      } else if (matchesForwardSync(e)) {
        e.preventDefault();
        a.forwardSync();
      } else if (zoom) {
        e.preventDefault();
        a.zoom(zoom);
      } else if (matchesMenuChord(e)) {
        const id = menuChordId(e);
        if (id) {
          e.preventDefault();
          a.menuAction(id);
        }
      } else if (!mod && e.key === '?') {
        a.showShortcuts();
      } else if (mod && e.key.toLowerCase() === 'b') {
        e.preventDefault();
        a.toggleTree();
      } else if (mod && e.key === 'Tab') {
        // Tab cycling when the tab strip is not focused; global fallback:
        const keys = [...buffersRef.current.keys()];
        if (keys.length > 1) {
          e.preventDefault();
          const i = keys.indexOf(fileNameRef.current);
          const n = e.shiftKey ? (i - 1 + keys.length) % keys.length : (i + 1) % keys.length;
          a.select(keys[n]);
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [act, buffersRef, fileNameRef]);
}
