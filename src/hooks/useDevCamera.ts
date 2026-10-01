import { useEffect } from 'react';
import { newMoves, type CameraMove } from '../lib/devCamera';
import { useLatest } from './useLatest';

// Dev builds only: polls the video harness's camera channel and hands each new
// move to `onMove` (src/lib/devCamera.ts).
export function useDevCamera(onMove: (m: CameraMove) => void) {
  const moveRef = useLatest(onMove);
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    let last = 0;
    let busy = false;
    const t = setInterval(() => {
      if (busy) return;
      busy = true;
      fetch('/__camera', { cache: 'no-store' })
        .then((r) => (r.ok ? r.text() : ''))
        .then((text) => {
          for (const m of newMoves(text, last)) {
            last = m.seq;
            moveRef.current(m);
          }
        })
        .catch(() => {})
        .finally(() => {
          busy = false;
        });
    }, 250);
    return () => clearInterval(t);
  }, [moveRef]);
}
