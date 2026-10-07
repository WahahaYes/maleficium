import { describe, it, expect } from 'vitest';
import temml from 'temml';
import { mathAt, mathSpans } from './mathSpans';

const tex = (doc: string) => mathSpans(doc).map((s) => [s.tex, s.display]);

describe('math spans', () => {
  it('finds every delimiter form with its range', () => {
    const doc = 'a $x^2$ b $$y$$ c \\(z\\) d \\[w\\] e';
    const spans = mathSpans(doc);
    expect(spans.map((s) => doc.slice(s.from, s.to))).toEqual([
      '$x^2$',
      '$$y$$',
      '\\(z\\)',
      '\\[w\\]',
    ]);
    expect(tex(doc)).toEqual([
      ['x^2', false],
      ['y', true],
      ['z', false],
      ['w', true],
    ]);
  });

  it('skips escapes, comments and verbatim', () => {
    expect(tex('costs \\$5 and \\$6')).toEqual([]);
    expect(tex('% $not math$\n$a$')).toEqual([['a', false]]);
    expect(tex('$a % comment $\n b$')).toEqual([['a % comment $\n b', false]]);
    expect(tex('\\begin{verbatim}$x$\\end{verbatim}$y$')).toEqual([['y', false]]);
  });

  it('stops inline math at a blank line, as TeX does', () => {
    expect(tex('a $ b\n\nc $d$')).toEqual([['d', false]]);
  });

  it('keeps environments renderable: no labels or numbers', () => {
    expect(tex('\\begin{equation}\\label{eq:a}E=mc^2\\end{equation}')).toEqual([['E=mc^2', true]]);
    expect(tex('\\begin{align}a&=b\\nonumber\\\\c&=d\\end{align}')).toEqual([
      ['\\begin{align*}a&=b\\\\c&=d\\end{align*}', true],
    ]);
    expect(tex('\\begin{eqnarray*}a&=&b\\end{eqnarray*}')).toEqual([
      ['\\begin{align*}a&=b\\end{align*}', true],
    ]);
  });

  it('finds the span under a caret, delimiters included', () => {
    const doc = 'x $a$ y $b$';
    const spans = mathSpans(doc);
    expect(mathAt(spans, 2)?.tex).toBe('a');
    expect(mathAt(spans, 5)?.tex).toBe('a');
    expect(mathAt(spans, 6)).toBeNull();
    expect(mathAt(spans, 10)?.tex).toBe('b');
    expect(mathAt([], 0)).toBeNull();
  });

  it('gives TeX the renderer reads without error', () => {
    const doc = [
      '$\\rightarrow$',
      '\\begin{equation}\\label{x} T(t) = T_s + (T_0 - T_s) e^{-kt} \\end{equation}',
      '\\begin{align}a&=b\\\\c&=d\\end{align}',
      '\\begin{gather*}x\\\\y\\end{gather*}',
      '\\begin{multline}a\\\\b\\end{multline}',
      '\\begin{eqnarray}a&=&b\\end{eqnarray}',
      '\\begin{alignat}{2}a&=b&c&=d\\end{alignat}',
    ].join('\n');
    for (const s of mathSpans(doc)) {
      expect(
        () => temml.renderToString(s.tex, { displayMode: s.display, throwOnError: true }),
        s.tex,
      ).not.toThrow();
    }
  });
});
