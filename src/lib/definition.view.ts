// definition.view.ts — wording for go-to-definition and its hover. Pure.

import type { Lookup } from './generated/index';

const KIND_WORD: Record<Lookup['ref']['kind'], string> = {
  label: 'label',
  citation: 'citation',
  macro: 'macro',
  input: 'file',
};

/** Hover text: the definition's summary and place, or that there is none.
 *  A macro the project does not define is TeX's or a package's: nothing to
 *  say, and in math the rendered formula is the hover instead. */
export function hoverText(l: Lookup): string | null {
  const [first, ...rest] = l.definitions;
  if (!first && l.ref.kind === 'macro') return null;
  if (!first) return `No definition for ${KIND_WORD[l.ref.kind]} ${l.ref.key}`;
  const more = rest.length > 0 ? ` (+${rest.length} more — duplicate)` : '';
  return `${first.summary}\n${first.rel}:${first.line}${more}`;
}
