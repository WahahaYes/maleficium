import { describe, expect, it } from 'vitest';
import { mainFileTip } from './mainFileTip';

describe('mainFileTip', () => {
  it('names the source of the pick', () => {
    expect(mainFileTip(null, 'scan')).toBe('No main file detected');
    expect(mainFileTip('/p/a.tex', 'config')).toBe('Main file (your choice): /p/a.tex');
    expect(mainFileTip('/p/a.tex', 'magic')).toContain('%!TEX root');
    expect(mainFileTip('/p/a.tex', 'scan')).toContain('auto-detected');
    expect(mainFileTip('/p/a.tex', 'single')).toContain('only .tex file');
    expect(mainFileTip('/p/a.tex', 'none')).toBe('Main file: /p/a.tex');
  });
});
