import { describe, it, expect } from 'vitest';
import { hoverText } from './definition.view';

describe('definition hover', () => {
  it('shows the summary and place, and flags duplicates', () => {
    const ref = { kind: 'label' as const, key: 'sec:a' };
    const def = { rel: 'ch/a.tex', line: 3, summary: '\\section{A}\\label{sec:a}' };
    expect(hoverText({ ref, definitions: [def] })).toBe('\\section{A}\\label{sec:a}\nch/a.tex:3');
    expect(hoverText({ ref, definitions: [def, def] })).toBe(
      '\\section{A}\\label{sec:a}\nch/a.tex:3 (+1 more — duplicate)',
    );
  });

  it('says plainly when nothing defines it', () => {
    expect(hoverText({ ref: { kind: 'citation', key: 'ghost' }, definitions: [] })).toBe(
      'No definition for citation ghost',
    );
    expect(
      hoverText({ ref: { kind: 'input', key: 'ch/x', command: 'input' }, definitions: [] }),
    ).toBe('No definition for file ch/x');
  });

  it('says nothing of a macro the project leaves to TeX or a package', () => {
    expect(hoverText({ ref: { kind: 'macro', key: '\\rightarrow' }, definitions: [] })).toBeNull();
  });
});
