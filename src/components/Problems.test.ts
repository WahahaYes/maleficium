import { describe, it, expect } from 'vitest';
import { parseLog } from './Problems';

describe('parseLog', () => {
  it('parses bare error with root=base=/tmp/x', () => {
    const result = parseLog('error: hello.tex:3: Undefined control sequence', '/tmp/x', '/tmp/x');
    expect(result).toEqual([
      { file: '/tmp/x/hello.tex', line: 3, msg: 'Undefined control sequence', clickable: true }
    ]);
  });

  it('parses ./parts/body.tex:12: oops with root /proj', () => {
    const result = parseLog('./parts/body.tex:12: oops', '/proj', '/proj');
    expect(result).toEqual([
      { file: '/proj/parts/body.tex', line: 12, msg: 'oops', clickable: true }
    ]);
  });

  it('parses absolute path /other/a.tex:5: msg with root /proj as non-clickable', () => {
    const result = parseLog('/other/a.tex:5: msg', '/proj', '/proj');
    expect(result).toEqual([
      { file: '/other/a.tex', line: 5, msg: 'msg', clickable: false }
    ]);
  });

  it('resolves a/b/../c.tex:7: x with root /tmp/x', () => {
    const result = parseLog('a/b/../c.tex:7: x', '/tmp/x', '/tmp/x');
    expect(result).toEqual([
      { file: '/tmp/x/a/c.tex', line: 7, msg: 'x', clickable: true }
    ]);
  });

  it('returns empty array for l.3 message alone', () => {
    const result = parseLog('l.3 Hello \\badcommand', '/tmp/x', '/tmp/x');
    expect(result).toEqual([]);
  });

  it('parses full tectonic-style snippet', () => {
    const result = parseLog(
      'error: hello.tex:3: Undefined control sequence\n' +
      'l.3 Hello \\badcommand\n' +
      'No pages of output.',
      '/tmp/x',
      '/tmp/x'
    );
    expect(result).toEqual([
      { file: '/tmp/x/hello.tex', line: 3, msg: 'Undefined control sequence', clickable: true }
    ]);
  });
});