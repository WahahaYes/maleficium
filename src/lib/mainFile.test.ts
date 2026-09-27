import { describe, it, expect } from 'vitest';
import { parseMagicComment, hasDocumentclass, resolveMainFile } from './mainFile';

describe('parseMagicComment', () => {
  it('parses %!TEX root', () => {
    expect(parseMagicComment('%!TEX root = main.tex\n\\input{body}')).toBe('main.tex');
  });
  it('tolerates spacing and quotes', () => {
    expect(parseMagicComment('% ! TEX root=  "ch/main.tex" ')).toBe('ch/main.tex');
  });
  it('returns null when absent', () => {
    expect(parseMagicComment('\\documentclass{article}')).toBeNull();
  });
});

describe('hasDocumentclass', () => {
  it('detects documentclass with options', () => {
    expect(hasDocumentclass('\\documentclass[11pt]{article}')).toBe(true);
  });
  it('rejects fragments', () => {
    expect(hasDocumentclass('\\section{Hi}')).toBe(false);
  });
});

describe('resolveMainFile order', () => {
  const texts: Record<string, string> = {
    '/r/main.tex': '\\documentclass{article}\n\\begin{document}hi\\end{document}\n',
    '/r/ch1.tex': '\\section{One}\n',
    '/r/ch2.tex': '%!TEX root = main.tex\n\\section{Two}\n',
  };
  const base = {
    root: '/r',
    readText: async (p: string) => {
      if (!(p in texts)) throw new Error('missing ' + p);
      return texts[p];
    },
    listTexFiles: async () => ['/r/main.tex', '/r/ch1.tex', '/r/ch2.tex'],
  };
  it('config wins', async () => {
    const r = await resolveMainFile({
      ...base,
      openedFile: '/r/ch2.tex',
      readConfig: async () => '{"mainFile":"main.tex"}',
    });
    expect(r).toMatchObject({ mainFile: '/r/main.tex', source: 'config' });
  });
  it('magic wins over scan', async () => {
    const r = await resolveMainFile({
      ...base,
      openedFile: '/r/ch2.tex',
      readConfig: async () => null,
    });
    expect(r).toMatchObject({ mainFile: '/r/main.tex', source: 'magic' });
  });
  it('scan finds documentclass deterministically', async () => {
    const r = await resolveMainFile({
      ...base,
      openedFile: '/r/ch1.tex',
      readConfig: async () => null,
    });
    expect(r).toMatchObject({ mainFile: '/r/main.tex', source: 'scan' });
  });
  it('single fallback', async () => {
    const r = await resolveMainFile({
      root: '/s',
      openedFile: null,
      readText: async () => '\\section{only}\n',
      listTexFiles: async () => ['/s/only.tex'],
      readConfig: async () => null,
    });
    expect(r).toMatchObject({ mainFile: '/s/only.tex', source: 'single' });
  });
  it('none when empty, never throws', async () => {
    const r = await resolveMainFile({
      root: '/e',
      openedFile: null,
      readText: async () => {
        throw new Error('no');
      },
      listTexFiles: async () => {
        throw new Error('no');
      },
      readConfig: async () => {
        throw new Error('no');
      },
    });
    expect(r).toMatchObject({ mainFile: null, source: 'none' });
  });
});

describe('resolveMainFile on Windows paths', () => {
  const root = 'C:\\Users\\u\\r';
  const texts: Record<string, string> = {
    'C:\\Users\\u\\r\\sub\\ch.tex': '%!TEX root = ../main.tex\n',
  };
  const deps = {
    root,
    readText: async (p: string) => texts[p] ?? '',
    listTexFiles: async () => [],
  };
  it('joins a relative config main onto the root', async () => {
    const r = await resolveMainFile({
      ...deps,
      openedFile: null,
      readConfig: async () => '{"mainFile":"sub/main.tex"}',
    });
    expect(r).toMatchObject({ mainFile: 'C:\\Users\\u\\r\\sub\\main.tex', source: 'config' });
  });
  it('keeps an absolute config main and resolves magic next to the opened file', async () => {
    const abs = await resolveMainFile({
      ...deps,
      openedFile: null,
      readConfig: async () => '{"mainFile":"D:\\\\other\\\\main.tex"}',
    });
    expect(abs.mainFile).toBe('D:\\other\\main.tex');
    const magic = await resolveMainFile({
      ...deps,
      openedFile: 'C:\\Users\\u\\r\\sub\\ch.tex',
      readConfig: async () => null,
    });
    expect(magic).toMatchObject({
      mainFile: 'C:\\Users\\u\\r\\sub\\..\\main.tex',
      source: 'magic',
    });
  });
});
