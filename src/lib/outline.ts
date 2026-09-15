// outline.ts — document outline from the ACTIVE buffer only.
//
// Growth cap: O(buffer) scan, debounced by the caller (500ms). Two caps with
// different owners, stated together so the next reader doesn't "fix" one:
// parse caps at 1000 (the DATA bound — full fidelity for search/next
// surfaces); the VIEW + Selection submenus cap at 100/25 (visible rows stay
// O(visible); the counted label makes the remainder honest, not hidden).
// Raising the submenu caps means paging or filtering them — never dumping
// unbounded rows into a menu (scale law #2).
//
// D-12 symbols: sections carry the hierarchy; labels, floats, and \input
// boundaries ride ALONG as non-hierarchical marker rows (they never change
// a section's level — the indent column stays a pure section tree, and the
// filter segments Sections/Labels/Figures/Inputs by marker kind).

/** What a row IS: a sectioning command, or a marker riding the tree. */
export type OutlineKind = 'section' | 'label' | 'figure' | 'table' | 'input';

export interface OutlineEntry {
  level: number;
  title: string;
  line: number;
  kind: OutlineKind;
  /** Marker detail: label key, env caption/file, or input path. Never shown raw. */
  detail?: string;
}

/** Filter segments for the outline view (All = interleaved, document order). */
export type OutlineFilter = 'all' | 'sections' | 'labels' | 'figures' | 'inputs';

export function filterOutline(entries: OutlineEntry[], filter: OutlineFilter): OutlineEntry[] {
  switch (filter) {
    case 'sections': return entries.filter((e) => e.kind === 'section');
    case 'labels': return entries.filter((e) => e.kind === 'label');
    case 'figures': return entries.filter((e) => e.kind === 'figure' || e.kind === 'table');
    case 'inputs': return entries.filter((e) => e.kind === 'input');
    default: return entries;
  }
}

const MAX_ENTRIES = 1000;

const SECTION_RE = /\\(chapter|section|subsection|subsubsection|paragraph)\*?\{([^}]{1,200})\}/g;
const LEVEL: Record<string, number> = {
  chapter: 0,
  section: 1,
  subsection: 2,
  subsubsection: 3,
  paragraph: 4,
};
// Markers: label keys (incl. inside captions — the label's own line wins),
// float environments (caption preferred, file fallback, type from env),
// \input boundaries (path only — contents stay in their own file's outline).
const LABEL_RE = /\\label\{([^}]{1,120})\}/g;
const FLOAT_RE = /\\begin\{(figure|table)\*?\}([\s\S]{0,2000}?)\\end\{\1\*?\}/g;
const CAPTION_RE = /\\caption(?:\[[^\]]*\])?\{([^}]{0,160})\}/;
const GRAPHICS_RE = /\\includegraphics(?:\[[^\]]*\])?\{([^}]{1,160})\}/;
const INPUT_RE = /\\(?:input|include)\{([^}]{1,160})\}/g;

function cleanTitle(raw: string): string {
  // Titles are display text: strip simple formatting commands (keep their
  // text), drop math spans and bare commands, collapse whitespace. Never
  // throws (regex-only, bounded input).
  return raw
    .replace(/\\(textbf|textit|textsc|textsf|texttt|emph|verb)\{([^}]*)\}/g, '$2')
    .replace(/\$[^$]{0,80}\$/g, '')
    .replace(/\\[a-zA-Z]+\s?/g, '')
    .replace(/[{}]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

function baseName(p: string): string {
  const s = p.split('/').pop() ?? p;
  return s || p;
}

export function parseOutline(text: string): OutlineEntry[] {
  const out: OutlineEntry[] = [];
  // Line numbers without a full split: track offsets of newlines lazily.
  const lineStarts: number[] = [0];
  const lineOf = (offset: number): number => {
    let lo = 0;
    let hi = lineStarts.length;
    while (lo + 1 < hi) {
      const mid = (lo + hi) >> 1;
      if (lineStarts[mid] <= offset) lo = mid;
      else hi = mid;
    }
    return lo + 1;
  };
  // Comments must not contribute headings: blank them (same length → offsets hold).
  const stripped = text.replace(/^%.*$/gm, (line) => ' '.repeat(line.length));
  // Build line index over stripped (same length as text).
  for (let i = 0; i < stripped.length; i++) {
    if (stripped[i] === '\n') lineStarts.push(i + 1);
  }
  const push = (e: OutlineEntry) => {
    if (out.length < MAX_ENTRIES) out.push(e);
  };
  // One pass per symbol class, merged by offset → single document-order list.
  // Sections own the hierarchy; markers take the CURRENT section level so the
  // indent column never invents structure (markers sort with their context).
  type Raw = { offset: number; entry: Omit<OutlineEntry, 'level'> & { level?: number } };
  const raws: Raw[] = [];
  let m: RegExpExecArray | null;
  SECTION_RE.lastIndex = 0;
  while ((m = SECTION_RE.exec(stripped)) !== null) {
    const title = cleanTitle(m[2]) || '(untitled)';
    raws.push({ offset: m.index, entry: { level: LEVEL[m[1]] ?? 1, title, line: lineOf(m.index), kind: 'section' } });
  }
  LABEL_RE.lastIndex = 0;
  while ((m = LABEL_RE.exec(stripped)) !== null) {
    raws.push({ offset: m.index, entry: { title: m[1].trim() || '(unlabeled)', line: lineOf(m.index), kind: 'label', detail: m[1].trim() } });
  }
  FLOAT_RE.lastIndex = 0;
  while ((m = FLOAT_RE.exec(stripped)) !== null) {
    const env = m[1] as 'figure' | 'table';
    const body = m[2] ?? '';
    const cap = body.match(CAPTION_RE)?.[1]?.trim() ?? '';
    const gfx = body.match(GRAPHICS_RE)?.[1]?.trim() ?? '';
    const title = cleanTitle(cap) || (gfx ? baseName(gfx) : `(${env})`);
    const detail = gfx || undefined;
    raws.push({ offset: m.index, entry: { title, line: lineOf(m.index), kind: env, detail } });
  }
  INPUT_RE.lastIndex = 0;
  while ((m = INPUT_RE.exec(stripped)) !== null) {
    const rel = m[1].trim();
    raws.push({ offset: m.index, entry: { title: baseName(rel), line: lineOf(m.index), kind: 'input', detail: rel } });
  }
  // Float-body labels were claimed by their float: drop label raws whose
  // offset falls inside a float span (they'd double-list the same key).
  const floatSpans: [number, number][] = [];
  FLOAT_RE.lastIndex = 0;
  while ((m = FLOAT_RE.exec(stripped)) !== null) {
    floatSpans.push([m.index, m.index + m[0].length]);
  }
  const inFloat = (off: number) => floatSpans.some(([a, b]) => off > a && off < b);
  raws.sort((a, b) => a.offset - b.offset);
  let currentLevel = 1;
  for (const r of raws) {
    if (r.entry.kind === 'label' && inFloat(r.offset)) continue;
    if (r.entry.kind === 'section') {
      currentLevel = r.entry.level ?? 1;
      push(r.entry as OutlineEntry);
    } else {
      push({ level: currentLevel, ...r.entry } as OutlineEntry);
    }
    if (out.length >= MAX_ENTRIES) break;
  }
  return out;
}
