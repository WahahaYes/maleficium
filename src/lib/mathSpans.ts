// mathSpans.ts — where the math is in a TeX source, for the editor's math
// preview. Pure.
//
// One pass over the text: `$…$`, `$$…$$`, `\(…\)`, `\[…\]` and the display
// math environments. Comments, escaped characters and verbatim environments
// are skipped, and inline `$` math stops at a blank line, as TeX's does.

/** One piece of math: its source range and the TeX a renderer takes. */
export interface MathSpan {
  /** Offset of the opening delimiter. */
  from: number;
  /** Offset just past the closing delimiter. */
  to: number;
  /** Delimiters stripped; an environment keeps its `\begin`/`\end`. */
  tex: string;
  display: boolean;
}

const MATH_ENVS = new Set(
  [
    'equation',
    'align',
    'gather',
    'multline',
    'flalign',
    'alignat',
    'eqnarray',
    'displaymath',
    'math',
  ].flatMap((e) => [e, `${e}*`]),
);
const VERBATIM_ENVS = new Set(['verbatim', 'verbatim*', 'lstlisting', 'minted', 'comment']);

const BEGIN = /\\begin\s*\{([a-zA-Z]+\*?)\}/y;

/** Every math span in `doc`, in order. */
export function mathSpans(doc: string): MathSpan[] {
  const out: MathSpan[] = [];
  let i = 0;
  while (i < doc.length) {
    const c = doc[i];
    if (c === '%') {
      i = lineEnd(doc, i);
      continue;
    }
    if (c === '\\') {
      const n = doc[i + 1];
      if (n === '(' || n === '[') {
        const close = n === '(' ? '\\)' : '\\]';
        const end = closing(doc, i + 2, close, false);
        if (end < 0) return out;
        out.push({
          from: i,
          to: end + 2,
          tex: doc.slice(i + 2, end),
          display: n === '[',
        });
        i = end + 2;
        continue;
      }
      BEGIN.lastIndex = i;
      const m = BEGIN.exec(doc);
      if (m) {
        const env = m[1];
        const endTag = `\\end{${env}}`;
        if (MATH_ENVS.has(env)) {
          const end = closing(doc, i + m[0].length, endTag, false);
          if (end < 0) return out;
          out.push({
            from: i,
            to: end + endTag.length,
            tex: displayTex(env, doc.slice(i + m[0].length, end)),
            display: env !== 'math' && env !== 'math*',
          });
          i = end + endTag.length;
          continue;
        }
        if (VERBATIM_ENVS.has(env)) {
          const end = doc.indexOf(endTag, i + m[0].length);
          if (end < 0) return out;
          i = end + endTag.length;
          continue;
        }
        i += m[0].length;
        continue;
      }
      i += 2; // a control symbol (`\$`, `\%`) or a command's backslash
      continue;
    }
    if (c === '$') {
      const display = doc[i + 1] === '$';
      const open = display ? 2 : 1;
      const end = closing(doc, i + open, display ? '$$' : '$', !display);
      if (end < 0) {
        // Unclosed: inline math ends at the paragraph; display math never does.
        if (display) return out;
        i += open;
        continue;
      }
      out.push({ from: i, to: end + open, tex: doc.slice(i + open, end), display });
      i = end + open;
      continue;
    }
    i++;
  }
  return out;
}

/** The span the caret at `pos` sits in (delimiters included), or null. */
export function mathAt(spans: readonly MathSpan[], pos: number): MathSpan | null {
  let lo = 0;
  let hi = spans.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const s = spans[mid];
    if (pos < s.from) hi = mid - 1;
    else if (pos > s.to) lo = mid + 1;
    else return s;
  }
  return null;
}

/** Offset of `close` from `i`, past comments and escapes; -1 when absent
 *  (or, for `stopAtPar`, when a blank line comes first). */
function closing(doc: string, i: number, close: string, stopAtPar: boolean): number {
  while (i < doc.length) {
    if (doc.startsWith(close, i)) return i;
    const c = doc[i];
    if (c === '\\') {
      i += 2;
      continue;
    }
    if (c === '%') {
      i = lineEnd(doc, i);
      continue;
    }
    if (stopAtPar && c === '\n' && /^\n[ \t]*\n/.test(doc.slice(i, i + 64))) return -1;
    i++;
  }
  return -1;
}

function lineEnd(doc: string, i: number): number {
  const nl = doc.indexOf('\n', i);
  return nl < 0 ? doc.length : nl;
}

/** An environment's body as TeX a renderer takes: numbering and labels are
 *  the PDF's business, and `eqnarray`'s three columns read as `align`'s. */
function displayTex(env: string, body: string): string {
  const clean = body.replace(/\\(?:label\s*\{[^{}]*\}|nonumber\b|notag\b)/g, '');
  const base = env.replace(/\*$/, '');
  if (base === 'equation' || base === 'displaymath' || base === 'math') return clean;
  if (base === 'eqnarray')
    return `\\begin{align*}${clean.replace(/&([^&\\]*)&/g, '&$1')}\\end{align*}`;
  return `\\begin{${base}*}${clean}\\end{${base}*}`;
}
