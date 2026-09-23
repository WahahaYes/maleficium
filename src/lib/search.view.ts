// search.view.ts — view model for the project search panel: match
// highlighting and the result summary. Pure, so the wording is pinned
// headlessly.

import type { Hit, SearchResult } from './generated/index';

/** A preview line split around its match: before, match, after. */
export function hitParts(hit: Hit): [string, string, string] {
  const start = Math.max(0, hit.col - hit.previewCol);
  const end = Math.min(hit.preview.length, start + hit.len);
  return [hit.preview.slice(0, start), hit.preview.slice(start, end), hit.preview.slice(end)];
}

/** One line under the search box saying what was found and what was not. */
export function searchSummary(r: SearchResult): string {
  const files = r.files.length;
  const head =
    r.hits === 0
      ? 'No results'
      : `${r.hits}${r.truncated > 0 ? '+' : ''} result${r.hits === 1 ? '' : 's'} in ${files} file${files === 1 ? '' : 's'}`;
  const parts = [head];
  if (r.truncated > 0) parts.push(`${r.truncated} more not shown — narrow the search`);
  if (r.unsearched > 0)
    parts.push(`${r.unsearched} file${r.unsearched === 1 ? '' : 's'} not searchable`);
  return parts.join(' · ');
}
