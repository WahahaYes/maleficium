// outline.ts — document outline from the ACTIVE buffer only.
//
// Growth cap: O(buffer) scan, debounced by the caller (500ms), capped entries.
// Own parse (ideas-only from StructureTreeView shape): sectioning commands →
// {level, title, line}. Never per keystroke (scale law #3).

export interface OutlineEntry {
  level: number;
  title: string;
  line: number;
}

const MAX_ENTRIES = 1000;

const RE = /\\(chapter|section|subsection|subsubsection|paragraph)\*?\{([^}]{1,200})\}/g;
const LEVEL: Record<string, number> = {
  chapter: 0,
  section: 1,
  subsection: 2,
  subsubsection: 3,
  paragraph: 4,
};

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
  let m: RegExpExecArray | null;
  RE.lastIndex = 0;
  // Comments must not contribute headings: blank them (same length → offsets hold).
  const stripped = text.replace(/^%.*$/gm, (line) => ' '.repeat(line.length));
  // Build line index over stripped (same length as text).
  for (let i = 0; i < stripped.length; i++) {
    if (stripped[i] === '\n') lineStarts.push(i + 1);
  }
  while ((m = RE.exec(stripped)) !== null) {
    out.push({ level: LEVEL[m[1]] ?? 1, title: m[2].trim() || '(untitled)', line: lineOf(m.index) });
    if (out.length >= MAX_ENTRIES) break;
  }
  return out;
}
