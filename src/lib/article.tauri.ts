// article.tauri.ts — desktop transport for the in-app article.
//
// The webview names a project and a main file only; the core exports the
// article and shelves the page under a token, served by the
// `maleficium-article` scheme. The frame loads that URL rather than a
// srcdoc, so the page runs under its own CSP meta, not the window's. A web
// build serves the same token over an HTTP route. Failures are plain strings
// from the core (an unknown root is refused without naming a path; a main
// file with no compiled pdf says to compile first).

import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import type { ArticleServed } from './generated/api';

/** The scheme `src-tauri/src/article.rs` registers. */
export const ARTICLE_SCHEME = 'maleficium-article';

/** Shelves the reader page and returns its token with the heading anchors. */
export function fetchArticle(rootId: string, mainRel: string): Promise<ArticleServed> {
  return invoke<ArticleServed>('article_bundle', { rootId, mainRel });
}

/** The frame URL of a shelved page (`http://<scheme>.localhost/` on Windows). */
export function articleSrc(token: string): string {
  return convertFileSrc(token, ARTICLE_SCHEME);
}
