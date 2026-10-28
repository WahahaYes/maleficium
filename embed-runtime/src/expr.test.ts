// Unit tests of the formula interpreter, an algorithmic kernel. The cases
// live in expr.vectors.json, shared with the .sty's l3fp path
// (e2e/liveeq-vectors.sh), so a case added there runs on both.
import { describe, expect, it } from 'vitest';
import { MAX_DEPTH, MAX_LENGTH, evaluate, format, parse, validParamName } from './expr';
import vectors from './expr.vectors.json';

describe('formula values', () => {
  for (const v of vectors.values) {
    it(`${JSON.stringify(v.expr)} at ${v.places} places`, () => {
      const vars = v.vars as Record<string, number>;
      const p = parse(v.expr, Object.keys(vars));
      if (!p.ok) throw new Error(`rejected at ${p.at}: ${p.message}`);
      expect(format(evaluate(p.expr, vars), v.places)).toBe(v.out);
    });
  }
});

describe('formula rejects', () => {
  for (const r of vectors.rejects) {
    it(JSON.stringify(r.expr), () => {
      const p = parse(r.expr, r.names);
      expect(p.ok).toBe(false);
      if (!p.ok) expect(p.at).toBeLessThanOrEqual(r.expr.length);
    });
  }
});

describe('hostile formulas', () => {
  it('refuses a formula past MAX_LENGTH', () => {
    expect(parse('1+'.repeat(MAX_LENGTH), []).ok).toBe(false);
  });
  it('refuses nesting past MAX_DEPTH instead of overflowing the stack', () => {
    const deep = '('.repeat(MAX_DEPTH + 1) + '1' + ')'.repeat(MAX_DEPTH + 1);
    expect(parse(deep, [])).toMatchObject({ ok: false, message: 'formula is nested too deeply' });
    const unary = '-'.repeat(MAX_DEPTH + 1) + '1';
    expect(parse(unary, []).ok).toBe(false);
  });
  it('points at the offending token', () => {
    expect(parse('1 + 2k', ['k'])).toMatchObject({ ok: false, at: 5 });
    expect(parse('a + b', ['a'])).toMatchObject({ ok: false, at: 4, message: "unknown name 'b'" });
  });
});

describe('param names', () => {
  it('takes letters then letters or digits, never a function or pi', () => {
    expect(['k', 'N0', 'tau', 'kAlpha2'].every(validParamName)).toBe(true);
    expect(['', '0k', 'k_1', 'pi', 'exp', 'round'].some(validParamName)).toBe(false);
  });
});
