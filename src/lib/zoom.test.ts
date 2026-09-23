import { describe, expect, it } from 'vitest';
import {
  applyZoom,
  DEFAULT_ZOOM,
  pageWidth,
  parseZoom,
  percentOf,
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
