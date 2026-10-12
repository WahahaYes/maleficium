import { useCallback, useEffect, useMemo, useState } from 'react';
import { emit } from '../lib/events';
import type { OutlineEntry } from '../lib/generated/structure';
import { structure } from '../lib/structure';

// The active buffer's outline, debounced 500ms. Full-fidelity entries
// (parse cap 1000); the view slices to 100 per filter (filter-first,
// cap-second). `sections` feeds the Selection submenus and article sync;
// `picks` is the Selection > Pick Sections multi-pick set.
export function useOutline(tex: string, fileName: string) {
  const [entries, setEntries] = useState<OutlineEntry[]>([]);
  const [picks, setPicks] = useState<number[]>([]);
  useEffect(() => {
    // A reply for a superseded buffer is dropped: parses resolve async.
    let stale = false;
    const t = setTimeout(() => {
      const t0 = performance.now();
      structure()
        .outline(tex)
        .then(({ entries }) => {
          if (stale) return;
          setEntries(entries);
          if (tex.length > 1_000_000) {
            const ms = Math.round(performance.now() - t0);
            emit({
              scope: 'app',
              kind: 'info',
              actor: 'system',
              message: `outline parsed ${entries.length} entries in ${ms}ms`,
              event: { action: 'outline.parse', entries: entries.length, ms },
            });
          }
        })
        .catch(() => {
          /* outline never blocks editing */
        });
    }, 500);
    return () => {
      stale = true;
      clearTimeout(t);
    };
  }, [tex, fileName]);
  const sections = useMemo(
    () =>
      entries.filter((o) => o.kind === 'section').map((o) => ({ line: o.line, title: o.title })),
    [entries],
  );
  const togglePick = useCallback(
    (line: number) =>
      setPicks((prev) => (prev.includes(line) ? prev.filter((l) => l !== line) : [...prev, line])),
    [],
  );
  return { entries, sections, picks, togglePick };
}
