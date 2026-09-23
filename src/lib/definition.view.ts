// definition.view.ts — wording for go-to-definition and its hover. Pure.

import type { Lookup } from './generated/index';

const KIND_WORD: Record<Lookup['ref']['kind'], string> = {
  label: 'label',
  citation: 'citation',
  macro: 'macro',
  input: 'file',
};

/** Hover text: the definition's summary and place, or that there is none. */
export function hoverText(l: Lookup): string {
  const [first, ...rest] = l.definitions;
  if (!first) return `No definition for ${KIND_WORD[l.ref.kind]} ${l.ref.key}`;
  const more = rest.length > 0 ? ` (+${rest.length} more — duplicate)` : '';
  return `${first.summary}\n${first.rel}:${first.line}${more}`;
}
