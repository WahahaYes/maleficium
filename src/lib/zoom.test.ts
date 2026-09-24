import { describe, expect, it } from 'vitest';
import {
  applyZoom,
  backingWidth,
  DEFAULT_ZOOM,
  MAX_RENDER_DPR,
  pageWidth,
  parseZoom,
  percentOf,
  renderGeometry,
  stalePages,
  ZOOM_GUTTER,
  ZOOM_MAX,
  ZOOM_MIN,
  zoomLabel,
} from './zoom';

const A4 = { width: 595, height: 842 };
const PANE = { width: 816, height: 600 };

describe('pageWidth', () => {
  it('fits the pane width, less the gutter', () => {
    expect(pageWidth({ kind: 'fit-width' }, PANE, A4)).toBe(PANE.width - ZOOM_GUTTER);
  });

  it('fits a whole page: height-bound in a short pane, width-bound in a narrow one', () => {
    const short = pageWidth({ kind: 'fit-page' }, PANE, A4);
    expect(short).toBeCloseTo((PANE.height - ZOOM_GUTTER) * (595 / 842));
    const narrow = pageWidth({ kind: 'fit-page' }, { width: 200, height: 2000 }, A4);
    expect(narrow).toBe(200 - ZOOM_GUTTER);
  });

  it('shows 100% at print size and clamps percent', () => {
    expect(pageWidth({ kind: 'percent', percent: 100 }, PANE, A4)).toBeCloseTo(595 * (96 / 72));
    expect(pageWidth({ kind: 'percent', percent: 1000 }, PANE, A4)).toBeCloseTo(
      595 * (96 / 72) * (ZOOM_MAX / 100),
    );
    expect(percentOf(pageWidth({ kind: 'percent', percent: 150 }, PANE, A4), A4)).toBeCloseTo(150);
  });
});

describe('applyZoom', () => {
  it('steps to the next multiple of 10 in each direction', () => {
    expect(applyZoom('in', 100)).toEqual({ kind: 'percent', percent: 110 });
    expect(applyZoom('in', 103.4)).toEqual({ kind: 'percent', percent: 110 });
    expect(applyZoom('out', 103.4)).toEqual({ kind: 'percent', percent: 100 });
    expect(applyZoom('out', 100)).toEqual({ kind: 'percent', percent: 90 });
  });

  it('clamps at the limits and switches to fit modes', () => {
    expect(applyZoom('out', ZOOM_MIN)).toEqual({ kind: 'percent', percent: ZOOM_MIN });
    expect(applyZoom('in', ZOOM_MAX)).toEqual({ kind: 'percent', percent: ZOOM_MAX });
    expect(applyZoom('fit-page', 180)).toEqual({ kind: 'fit-page' });
    expect(applyZoom('fit-width', 180)).toEqual({ kind: 'fit-width' });
  });
});

describe('zoom pref', () => {
  it('round-trips and rejects malformed values', () => {
    for (const m of [
      { kind: 'fit-width' },
      { kind: 'fit-page' },
      { kind: 'percent', percent: 130 },
    ] as const) {
      expect(parseZoom(JSON.stringify(m))).toEqual(m);
    }
    expect(parseZoom(null)).toEqual(DEFAULT_ZOOM);
    expect(parseZoom('{nope')).toEqual(DEFAULT_ZOOM);
    expect(parseZoom('{"kind":"percent","percent":"x"}')).toEqual(DEFAULT_ZOOM);
    expect(parseZoom('{"kind":"percent","percent":9000}')).toEqual({
      kind: 'percent',
      percent: ZOOM_MAX,
    });
  });

  it('labels fit modes with the percent shown', () => {
    expect(zoomLabel({ kind: 'fit-width' }, 123.4)).toBe('Fit width (123%)');
    expect(zoomLabel({ kind: 'percent', percent: 90 }, 90)).toBe('90%');
  });
});

describe('renderGeometry', () => {
  const percents: number[] = [];
  for (let p = ZOOM_MIN; p <= ZOOM_MAX; p += 5) percents.push(p);

  it('backs every zoom level 1:1 with whole device pixels, filled edge to edge', () => {
    for (const dpr of [1, 1.25, 1.5, 2]) {
      for (const p of percents) {
        const css = pageWidth({ kind: 'percent', percent: p }, PANE, A4);
        const g = renderGeometry(css, A4, dpr);
        expect(g.width, `${p}% @${dpr}`).toBe(Math.round(css * dpr));
        expect(Number.isInteger(g.height)).toBe(true);
        // The pdf.js viewport spans exactly the backing store: no resampled edge.
        expect(g.scale * A4.width).toBeCloseTo(g.width, 9);
        expect(Math.abs(g.scale * A4.height - g.height)).toBeLessThanOrEqual(0.5);
        // Backing px per displayed CSS px is the DPR on both axes.
        expect(Math.abs(g.width / css - dpr)).toBeLessThan(0.5 / css + 1e-9);
        // Height is within the two roundings (width, then height) of exact.
        const exactHeight = css * dpr * (A4.height / A4.width);
        expect(Math.abs(g.height - exactHeight)).toBeLessThanOrEqual(
          0.5 * (A4.height / A4.width) + 0.5 + 1e-9,
        );
      }
    }
  });

  it('gives every zoom step a new backing width, so each step re-renders', () => {
    for (let i = 1; i < percents.length; i++) {
      const a = pageWidth({ kind: 'percent', percent: percents[i - 1] }, PANE, A4);
      const b = pageWidth({ kind: 'percent', percent: percents[i] }, PANE, A4);
      expect(backingWidth(b, 1)).not.toBe(backingWidth(a, 1));
    }
  });

  it('caps the DPR and tolerates degenerate input', () => {
    expect(backingWidth(500, 3)).toBe(500 * MAX_RENDER_DPR);
    expect(backingWidth(500, 0)).toBe(500);
    expect(backingWidth(500, Number.NaN)).toBe(500);
    expect(backingWidth(0, 1)).toBe(1);
    const g = renderGeometry(300.4, { width: 0, height: 0 }, 1);
    expect(g).toEqual({ scale: 300, width: 300, height: 300 });
  });
});

describe('stalePages', () => {
  const painted = (css: number, dpr: number) =>
    [1, 2, 3].map((page) => ({ page, cssWidth: css, backing: backingWidth(css, dpr) }));

  it('keeps every bitmap when nothing resized (scrolling, a held fit)', () => {
    expect(stalePages(painted(517, 1), 1)).toEqual([]);
    expect(stalePages(painted(401.03, 1.25), 1.25)).toEqual([]);
    // Sub-pixel layout jitter that rounds to the same backing is not a change.
    expect(stalePages([{ page: 1, cssWidth: 517.3, backing: 517 }], 1)).toEqual([]);
  });

  it('marks only the pages whose width changed', () => {
    const mixed = [...painted(517, 1).slice(0, 2), { page: 3, cssWidth: 600, backing: 517 }];
    expect(stalePages(mixed, 1)).toEqual([3]);
  });

  it('marks every page when the DPR changes', () => {
    expect(stalePages(painted(517, 1), 2)).toEqual([1, 2, 3]);
  });
});
