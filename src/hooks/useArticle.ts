// useArticle.ts — the Article tab's bytes.
//
// Loads the reader bundle for the open main file on demand. Refresh is
// manual (the Reload control): a compile-phase auto-refresh, anchor sync
// and the approve round-trip arrive later.

import { useCallback, useState } from 'react';
import { fetchArticle } from '../lib/article.tauri';
import type { ArticleAnchor } from '../lib/generated/api';

export function useArticle() {
  const [html, setHtml] = useState<string | null>(null);
  const [anchors, setAnchors] = useState<ArticleAnchor[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (rootId: string, mainRel: string) => {
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

  return { html, anchors, loading, error, load };
}
