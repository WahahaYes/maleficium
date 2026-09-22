import { makeTmpRoot, makeSingle, makeMulti, makeImageDoc } from './fixtures';
import { emit } from '../lib/events';
import { transport } from '../lib/event-transport';
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
  it('5000 bus emits stay capped at 500', () => {
    transport().clear();
    for (let i = 0; i < 5000; i++) {
      emit({
        scope: 'compile',
        kind: 'info',
        actor: 'system',
        message: `m${i}`,
        event: { action: 'file.load', path: 'p' },
      });
    }
    expect(transport().snapshot().length).toBe(500);
    expect(transport().snapshot()[0].message).toBe('m4500');
  });
});
