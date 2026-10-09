// article.tauri.ts — desktop transport for the in-app article.
//
// The webview names a project and a main file only; the returned view
// carries no path. Failures are plain strings from the core (an unknown
// root is refused without naming a path; a main file with no compiled pdf
// says to compile first).

import { invoke } from '@tauri-apps/api/core';
import type { ArticleView } from './generated/api';

/** The reader bytes with their heading anchors, for the Article tab. */
export function fetchArticle(rootId: string, mainRel: string): Promise<ArticleView> {
  return invoke<ArticleView>('article_bundle', { rootId, mainRel });
}
