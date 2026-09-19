import { describe, it, expect } from 'vitest';
import { parseOutline, filterOutline, searchOutline } from './outline';

describe('parseOutline sections', () => {
  it('finds section hierarchy with line numbers', () => {
    const text =
      '\\documentclass{article}\n\\begin{document}\n\\section{Intro}\nHi\n\\subsection{Bits}\nX\n\\end{document}\n';
    expect(parseOutline(text)).toEqual([
      { level: 1, title: 'Intro', line: 3, kind: 'section' },
      { level: 2, title: 'Bits', line: 5, kind: 'section' },
    ]);
  });
  it('ignores commented sections', () => {
    expect(parseOutline('% \\section{Fake}\n\\section{Real}\n')).toEqual([
      { level: 1, title: 'Real', line: 2, kind: 'section' },
    ]);
  });
  it('strips formatting from titles', () => {
    expect(parseOutline('\\section{Intro \\textbf{bold}}\n')[0].title).toBe('Intro bold');
  });
});

describe('parseOutline symbols', () => {
  it('lists labels with their keys in document order', () => {
    const text = '\\section{Intro}\n\\label{sec:intro}\nText \\ref{fig:a}.\n';
    expect(parseOutline(text)).toEqual([
      { level: 1, title: 'Intro', line: 1, kind: 'section' },
      { level: 1, title: 'sec:intro', line: 2, kind: 'label', detail: 'sec:intro' },
    ]);
  });
  it('prefers float captions, falls back to file name', () => {
    const text =
      '\\begin{figure}[h]\n\\centering\n\\includegraphics[width=0.5\\textwidth]{figs/diagram}\n\\caption{A diagram.}\n\\label{fig:diagram}\n\\end{figure}\n';
    const rows = parseOutline(text);
    expect(rows.length).toBe(1);
    expect(rows[0]).toMatchObject({ kind: 'figure', title: 'A diagram.', detail: 'figs/diagram' });
    const noCap = '\\begin{figure}\n\\includegraphics{figs/photo}\n\\end{figure}\n';
    expect(parseOutline(noCap)[0]).toMatchObject({ kind: 'figure', title: 'photo' });
  });
  it('lists input boundaries with paths', () => {
    const text = '\\section{A}\n\\input{chapters/background}\n\\include{ch/method}\n';
    expect(parseOutline(text)).toEqual([
      { level: 1, title: 'A', line: 1, kind: 'section' },
      { level: 1, title: 'background', line: 2, kind: 'input', detail: 'chapters/background' },
      { level: 1, title: 'method', line: 3, kind: 'input', detail: 'ch/method' },
    ]);
  });
  it('marks markers with the current section level, never restructures', () => {
    const text = '\\section{A}\n\\label{a}\n\\subsection{B}\n\\label{b}\n';
    expect(parseOutline(text).map((e) => [e.kind, e.level])).toEqual([
      ['section', 1],
      ['label', 1],
      ['section', 2],
      ['label', 2],
    ]);
  });
  it('ignores commented and float-internal labels (no double list)', () => {
    const text = '% \\label{fake}\n\\begin{figure}\n\\caption{C}\n\\label{fig:c}\n\\end{figure}\n';
    const rows = parseOutline(text);
    expect(rows.map((e) => e.kind)).toEqual(['figure']);
  });
});

describe('filterOutline', () => {
  it('segments by kind, all stays document order', () => {
    const text =
      '\\section{A}\n\\label{a}\n\\begin{figure}\n\\caption{C}\n\\end{figure}\n\\input{ch/b}\n';
    const all = parseOutline(text);
    expect(all.map((e) => e.kind)).toEqual(['section', 'label', 'figure', 'input']);
    expect(filterOutline(all, 'sections').map((e) => e.kind)).toEqual(['section']);
    expect(filterOutline(all, 'labels').map((e) => e.kind)).toEqual(['label']);
    expect(filterOutline(all, 'figures').map((e) => e.kind)).toEqual(['figure']);
    expect(filterOutline(all, 'inputs').map((e) => e.kind)).toEqual(['input']);
    expect(filterOutline(all, 'all')).toEqual(all);
  });
  it('tables ride the figures segment', () => {
    const text = '\\begin{table}\n\\caption{T}\n\\end{table}\n';
    const all = parseOutline(text);
    expect(all[0].kind).toBe('table');
    expect(filterOutline(all, 'figures').length).toBe(1);
    expect(filterOutline(all, 'labels').length).toBe(0);
  });
  it('3000 sections parse and cap at 1000', () => {
    const lines = ['\\documentclass{article}', '\\begin{document}'];
    for (let i = 0; i < 3000; i++) lines.push(`\\section{S${i}} \\label{s:${i}}`);
    lines.push('\\end{document}');
    const rows = parseOutline(lines.join('\n'));
    expect(rows.length).toBe(1000);
  });
});

describe('searchOutline', () => {
  it('matches titles and details, case-insensitive', () => {
    const all = parseOutline(
      '\\section{Intro}\n\\label{sec:intro}\n\\begin{figure}\n\\caption{A diagram.}\n\\end{figure}\n',
    );
    expect(searchOutline(all, '').length).toBe(3);
    expect(searchOutline(all, 'intro').map((e) => e.kind)).toEqual(['section', 'label']);
    expect(searchOutline(all, 'SEC:INTRO').length).toBe(1);
    expect(searchOutline(all, 'diagram').map((e) => e.kind)).toEqual(['figure']);
    expect(searchOutline(all, 'nothing-here').length).toBe(0);
  });
});
