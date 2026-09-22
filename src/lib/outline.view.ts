// outline.view.ts — view-side narrowing of outline rows (search + kind
// segments). Pure; parsing lives behind the structure seam.

import type { OutlineEntry } from './generated/structure';

/** Filter segments for the outline view (All = interleaved, document order). */
export type OutlineFilter = 'all' | 'sections' | 'labels' | 'figures' | 'inputs';

/** Text search over outline rows (title + detail, case-insensitive). Pure. */
export function searchOutline(entries: OutlineEntry[], query: string): OutlineEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return entries;
  return entries.filter(
    (e) => e.title.toLowerCase().includes(q) || (e.detail ?? '').toLowerCase().includes(q),
  );
}

export function filterOutline(entries: OutlineEntry[], filter: OutlineFilter): OutlineEntry[] {
  switch (filter) {
    case 'sections':
      return entries.filter((e) => e.kind === 'section');
    case 'labels':
      return entries.filter((e) => e.kind === 'label');
    case 'figures':
      return entries.filter((e) => e.kind === 'figure' || e.kind === 'table');
    case 'inputs':
      return entries.filter((e) => e.kind === 'input');
    default:
      return entries;
  }
}
