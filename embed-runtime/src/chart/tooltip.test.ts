import { describe, expect, it } from 'vitest';
import { MAX_KEY, MAX_ROWS, MAX_VALUE, placeTip, tipContent } from './tooltip';
import { compileSpec, createView } from './core';

describe('chart tooltip content', () => {
  it('shows nothing for no value', () => {
    for (const v of [null, undefined, '', [], {}, { a: {} }]) expect(tipContent(v)).toBeNull();
  });
  it('shows a lone value as the title and an object as rows', () => {
    expect(tipContent(3)).toEqual({ title: '3', rows: [] });
    expect(tipContent({ title: 'alpha', score: 10, ok: true, nested: { x: 1 } })).toEqual({
      title: 'alpha',
      rows: [
        ['score', '10'],
        ['ok', 'true'],
      ],
    });
  });
  it('caps the fields and the text', () => {
    const many = Object.fromEntries(Array.from({ length: 40 }, (_, i) => [`f${i}`, i]));
    expect(tipContent(many)!.rows).toHaveLength(MAX_ROWS);
    const c = tipContent({ ['k'.repeat(500)]: 'v'.repeat(5000) })!;
    expect(c.rows[0][0]).toHaveLength(MAX_KEY);
    expect(c.rows[0][1]).toHaveLength(MAX_VALUE);
    expect(c.rows[0][1].endsWith('…')).toBe(true);
  });
  it('keeps markup as text', () => {
    // The DOM side sets textContent only; the content passes markup through
    // unchanged as a string, never parsed.
    expect(tipContent('<img src=x onerror=alert(1)>')!.title).toBe('<img src=x onerror=alert(1)>');
  });
});

describe('chart tooltip placement', () => {
  it('goes below right of the pointer', () => {
    expect(placeTip(10, 10, 50, 20, 400, 300)).toEqual({ left: 22, top: 22 });
  });
  it('flips at the edges and stays inside the viewport', () => {
    expect(placeTip(390, 290, 50, 20, 400, 300)).toEqual({ left: 328, top: 258 });
    const p = placeTip(5, 5, 500, 400, 400, 300);
    expect(p.left).toBeGreaterThanOrEqual(4);
    expect(p.top).toBeGreaterThanOrEqual(4);
  });
});

describe('chart tooltip wiring', () => {
  it('gives the view a tooltip for every mark by default', async () => {
    const view = createView(
      compileSpec(
        {
          data: { values: [{ a: 'x', b: 2 }] },
          mark: 'bar',
          encoding: { x: { field: 'a' }, y: { field: 'b', type: 'quantitative' } },
        },
        {},
        { width: 200, height: 100 },
      ),
    );
    await view.runAsync();
    const items: unknown[] = [];
    const walk = (s: { items?: unknown[]; tooltip?: unknown }) => {
      if (s.tooltip !== undefined) items.push(s.tooltip);
      for (const c of s.items ?? []) walk(c as never);
    };
    walk((view.scenegraph() as unknown as { root: never }).root);
    expect(items).toContainEqual({ a: 'x', b: '2' });
    view.finalize();
  });
});
