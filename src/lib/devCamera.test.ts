import { describe, expect, it } from 'vitest';
import { newMoves } from './devCamera';

describe('newMoves', () => {
  const text = [
    '{"seq":1,"op":"open","rel":"main.tex"}',
    '{"seq":2,"op":"line","line":40}',
    'not json',
    '{"seq":3,"op":"command","id":"tools.forward-sync"}',
    '{"seq":4,"op":"page","page":2}',
    '{"seq":5,"op":"command","id":"file.save"}',
    '{"seq":6,"op":"open","rel":"../outside.tex"}',
    '{"seq":7,"op":"open","rel":"/etc/passwd"}',
    '{"seq":8,"op":"line","line":0}',
    '{"seq":9,"op":"type","text":"x"}',
  ].join('\n');

  it('returns only well-formed moves after the last one run', () => {
    expect(newMoves(text, 0).map((m) => m.seq)).toEqual([1, 2, 3, 4]);
    expect(newMoves(text, 2).map((m) => m.seq)).toEqual([3, 4]);
    expect(newMoves(text, 4)).toEqual([]);
  });

  it('allows only view commands and project-relative files, never text', () => {
    const ops = newMoves(text, 0).map((m) => m.op);
    expect(ops).not.toContain('type');
    expect(newMoves('{"seq":1,"op":"command","id":"file.save"}', 0)).toEqual([]);
    expect(newMoves('{"seq":1,"op":"command","id":"edit.delete"}', 0)).toEqual([]);
  });
});
