import { makeTmpRoot, makeSingle, makeMulti, makeImageDoc } from './fixtures';
import { parseLog } from '../lib/parseLog';
import { emit, list, clear } from '../lib/events';
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { describe, it, expect } from 'vitest';

describe('scale fixtures', () => {
  it('single 50pp fixture builds', () => {
    const root = makeTmpRoot();
    const docPath = makeSingle(root, 50);
    expect(existsSync(docPath)).toBe(true);
    expect(readFileSync(docPath, 'utf-8')).toContain('\\newpage');
  });

  it('multi 20 chapters builds', () => {
    const root = makeTmpRoot();
    const mainPath = makeMulti(root, 20);
    expect(readFileSync(mainPath, 'utf-8')).toContain('\\input{ch1}');
    expect(existsSync(join(root, 'ch1.tex'))).toBe(true);
  });

  it('image doc 10 figs builds', () => {
    const root = makeTmpRoot();
    const mainPath = makeImageDoc(root, 10);
    expect(readFileSync(mainPath, 'utf-8')).toContain('\\includegraphics');
    const fig = readFileSync(join(root, 'fig1.png'));
    expect([...fig.subarray(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  });
});

describe('scale budgets', () => {
  it('parseLog over 20000-line log completes <2000ms', () => {
    const lines: string[] = [];
    for (let i = 1; i <= 20000; i++) {
      lines.push(i % 5 === 0 ? `error: ch${i}.tex:${i}: msg${i}` : `filler line ${i}`);
    }
    const start = Date.now();
    const entries = parseLog(lines.join('\n'), '/tmp/scale', '/tmp/scale');
    const elapsed = Date.now() - start;
    console.log(`parseLog 20k lines: ${elapsed}ms, ${entries.length} entries`);
    expect(entries.length).toBe(4000);
    expect(elapsed).toBeLessThan(2000);
  });

  it('5000 bus emits stay capped at 500', () => {
    clear();
    const start = Date.now();
    for (let i = 0; i < 5000; i++) {
      emit({ scope: 'compile', kind: 'info', message: `m${i}` });
    }
    const elapsed = Date.now() - start;
    console.log(`5000 emits: ${elapsed}ms`);
    expect(list().length).toBe(500);
    expect(list()[0].message).toBe('m4500');
  });
});
