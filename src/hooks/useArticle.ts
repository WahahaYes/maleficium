// useArticle.ts — the Article tab's bytes.
//
// Loads the reader bundle for the open main file on demand. The approve
// round-trip reloads through the same loader: every export re-judges the
// approval gate, so an approval settled in the Widgets panel shows on the
// next load. A finished compile auto-refreshes through the preview stamp.

import { useCallback, useRef, useState } from 'react';
import { fetchArticle } from '../lib/article.tauri';
import { emit } from '../lib/events';
import type { ArticleAnchor } from '../lib/generated/api';

export function useArticle() {
  const [html, setHtml] = useState<string | null>(null);
  const [anchors, setAnchors] = useState<ArticleAnchor[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** The preview stamp the shown bytes were exported from (0: never loaded). */
  const [stamp, setStamp] = useState(0);
  const argsRef = useRef<{ rootId: string; mainRel: string; stamp: number } | null>(null);

  const load = useCallback(async (rootId: string, mainRel: string, atStamp = 0) => {
    argsRef.current = { rootId, mainRel, stamp: atStamp };
    setStamp(atStamp);
    setLoading(true);
    setError(null);
    try {
      const view = await fetchArticle(rootId, mainRel);
      setHtml(view.html);
      setAnchors(view.anchors);
      emit({
        scope: 'preview',
        kind: 'success',
        actor: 'user',
        message: `article loaded (${view.anchors.length} sections)`,
        event: { action: 'article.load', anchors: view.anchors.length },
      });
    } catch (e) {
      const failure = String(e);
      setError(failure);
      emit({
        scope: 'preview',
        kind: 'error',
        actor: 'user',
        message: `article failed to load (${failure.slice(0, 120)})`,
        event: { action: 'article.load-failed', error: failure.slice(0, 200) },
      });
    } finally {
      setLoading(false);
    }
  }, []);

  // Re-export + re-render after an approval settles: a no-op until the
  // first load names its project and main file.
  const reload = useCallback(() => {
    const a = argsRef.current;
    if (a) void load(a.rootId, a.mainRel, a.stamp);
  }, [load]);

  return { html, anchors, loading, error, stamp, load, reload };
}
