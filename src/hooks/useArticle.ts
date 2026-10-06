// useArticle.ts — the Article tab's bytes.
//
// Loads the reader bundle for the open main file on demand. The approve
// round-trip reloads through the same loader: every export re-judges the
// approval gate, so an approval settled in the Widgets panel shows on the
// next load. Auto-refresh on the compile stamp arrives later.

import { useCallback, useRef, useState } from 'react';
import { fetchArticle } from '../lib/article.tauri';
import type { ArticleAnchor } from '../lib/generated/api';

export function useArticle() {
  const [html, setHtml] = useState<string | null>(null);
  const [anchors, setAnchors] = useState<ArticleAnchor[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const argsRef = useRef<{ rootId: string; mainRel: string } | null>(null);

  const load = useCallback(async (rootId: string, mainRel: string) => {
    argsRef.current = { rootId, mainRel };
    setLoading(true);
    setError(null);
    try {
      const view = await fetchArticle(rootId, mainRel);
      setHtml(view.html);
      setAnchors(view.anchors);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  // Re-export + re-render after an approval settles: a no-op until the
  // first load names its project and main file.
  const reload = useCallback(() => {
    const a = argsRef.current;
    if (a) void load(a.rootId, a.mainRel);
  }, [load]);

  return { html, anchors, loading, error, load, reload };
}
