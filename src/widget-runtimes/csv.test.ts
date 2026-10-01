import { describe, expect, it } from 'vitest';
import { detectDelimiter, parseCsv, toNumber } from './csv';

describe('parseCsv', () => {
  it('reads a header and rows', () => {
    expect(parseCsv('a,b\n1,2\n3,4\n')).toEqual({
      header: ['a', 'b'],
      rows: [
        ['1', '2'],
        ['3', '4'],
      ],
    });
  });
  it('handles quotes, escaped quotes, embedded delimiters and newlines, CRLF and a BOM', () => {
    const p = parseCsv('﻿name,note\r\n"a, b","say ""hi"""\r\n"x\ny",z');
    expect(p.header).toEqual(['name', 'note']);
    expect(p.rows).toEqual([
      ['a, b', 'say "hi"'],
      ['x\ny', 'z'],
    ]);
  });
  it('pads and cuts ragged rows to the header width', () => {
    expect(parseCsv('a,b\n1\n1,2,3').rows).toEqual([
      ['1', ''],
      ['1', '2'],
    ]);
  });
  it('keeps an empty quoted field and skips blank lines', () => {
    expect(parseCsv('a,b\n\n"",x\n').rows).toEqual([['', 'x']]);
  });
  it('is empty for empty input', () => {
    expect(parseCsv('')).toEqual({ header: [], rows: [] });
  });
  it('detects tab and semicolon delimiters outside quotes', () => {
    expect(detectDelimiter('a\tb\tc\n')).toBe('\t');
    expect(detectDelimiter('a;b;c\n')).toBe(';');
    expect(detectDelimiter('"a;b;c",d\n')).toBe(',');
    expect(parseCsv('a;b\n1;2').rows).toEqual([['1', '2']]);
  });
});

describe('toNumber', () => {
  it('accepts plain decimals only', () => {
    expect(toNumber(' 12.5 ')).toBe(12.5);
    expect(toNumber('-3e2')).toBe(-300);
    expect(toNumber('.5')).toBe(0.5);
    for (const s of ['', 'abc', '1,000', '0x10', 'Infinity', '12px'])
      expect(toNumber(s)).toBeNull();
  });
});
