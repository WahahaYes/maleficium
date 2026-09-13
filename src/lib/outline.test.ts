import { describe, it, expect } from 'vitest';
import { parseOutline } from './outline';

describe('parseOutline', () => {
  it('finds section hierarchy with line numbers', () => {
    const text = '\\documentclass{article}\n\\begin{document}\n\\section{Intro}\nHi\n\\subsection{Bits}\nX\n\\end{document}\n';
    expect(parseOutline(text)).toEqual([
      { level: 1, title: 'Intro', line: 3 },
      { level: 2, title: 'Bits', line: 5 },
    ]);
  });
  it('ignores commented sections and caps entries', () => {
    expect(parseOutline('% \\section{Fake}\n\\section{Real}\n')).toEqual([
      { level: 1, title: 'Real', line: 2 },
    ]);
  });
});
