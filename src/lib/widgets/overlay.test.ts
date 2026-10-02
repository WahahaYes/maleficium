import { describe, expect, it } from 'vitest';
import { pageWidth, type ZoomMode } from '../zoom';
import {
  centreDistance,
  intersect,
  nearPane,
  placement,
  rectOfSlot,
  slotInPage,
  slotPercent,
  toDevice,
  type CssBox,
} from './overlay';

const LETTER = { width: 612, height: 792 };
const A4 = { width: 595, height: 842 };
// One inch in from the left, top at 72 pt below the page top, 468 x 120 pt.
const RECT = { x0: 72, y0: 600, x1: 540, y1: 720 };
const PANE = { width: 816, height: 600 };

const box = (mode: ZoomMode, page = LETTER, pane = PANE) =>
  slotInPage(RECT, page, pageWidth(mode, pane, page));

describe('slotInPage across zoom modes', () => {
  it('maps at print size: 100% is 96/72 px per point', () => {
    expect(box({ kind: 'percent', percent: 100 })).toEqual({
      left: 96,
      top: 96,
      width: 624,
      height: 160,
    });
  });

  it('scales linearly with the percent', () => {
    expect(box({ kind: 'percent', percent: 50 })).toEqual({
      left: 48,
      top: 48,
      width: 312,
      height: 80,
    });
    expect(box({ kind: 'percent', percent: 200 })).toEqual({
      left: 192,
      top: 192,
      width: 1248,
      height: 320,
    });
  });

  it('follows the pane in fit-width', () => {
    const b = box({ kind: 'fit-width' });
    const s = 800 / 612;
    expect(b.left).toBeCloseTo(72 * s, 9);
    expect(b.top).toBeCloseTo(72 * s, 9);
    expect(b.width).toBeCloseTo(468 * s, 9);
    expect(b.height).toBeCloseTo(120 * s, 9);
  });

  it('follows the pane height in fit-page', () => {
    const b = box({ kind: 'fit-page' });
    const s = ((600 - 16) * (612 / 792)) / 612;
    expect(b.left).toBeCloseTo(72 * s, 9);
    expect(b.top).toBeCloseTo(72 * s, 9);
    expect(b.height).toBeCloseTo(120 * s, 9);
  });

  it('keeps the aspect of the rect and stays inside the page at every zoom', () => {
    const modes: ZoomMode[] = [
      { kind: 'fit-width' },
      { kind: 'fit-page' },
      ...[25, 33, 67, 100, 125, 175, 400].map((percent) => ({ kind: 'percent' as const, percent })),
    ];
    for (const page of [LETTER, A4]) {
      for (const pane of [PANE, { width: 300, height: 1400 }]) {
        for (const mode of modes) {
          const w = pageWidth(mode, pane, page);
          const b = slotInPage(RECT, page, w);
          const h = (w * page.height) / page.width;
          expect(b.width / b.height).toBeCloseTo(468 / 120, 9);
          expect(b.left).toBeGreaterThanOrEqual(0);
          expect(b.top).toBeGreaterThanOrEqual(0);
          expect(b.left + b.width).toBeLessThanOrEqual(w + 1e-9);
          expect(b.top + b.height).toBeLessThanOrEqual(h + 1e-9);
          const back = rectOfSlot(b, page, w);
          for (const k of ['x0', 'y0', 'x1', 'y1'] as const)
            expect(back[k]).toBeCloseTo(RECT[k], 9);
        }
      }
    }
  });

  it('puts a rect at the page bottom-left corner at the box bottom-left', () => {
    const b = slotInPage({ x0: 0, y0: 0, x1: 61.2, y1: 79.2 }, LETTER, 612);
    expect(b.left).toBe(0);
    expect(b.top + b.height).toBeCloseTo(792, 9);
  });
});

describe('slotPercent', () => {
  it('agrees with slotInPage at every page width', () => {
    const pct = slotPercent(RECT, LETTER);
    for (const w of [300, 612, 816, 1700]) {
      const b = slotInPage(RECT, LETTER, w);
      const h = (w * LETTER.height) / LETTER.width;
      expect((pct.left / 100) * w).toBeCloseTo(b.left, 9);
      expect((pct.top / 100) * h).toBeCloseTo(b.top, 9);
      expect((pct.width / 100) * w).toBeCloseTo(b.width, 9);
      expect((pct.height / 100) * h).toBeCloseTo(b.height, 9);
    }
  });
});

describe('device placement', () => {
  const pane: CssBox = { left: 0, top: 40, width: 800, height: 500 };

  it('rounds to whole device px and scales by the dpr', () => {
    expect(toDevice({ left: 10.4, top: 20.6, width: 100.2, height: 50 }, 1)).toEqual({
      x: 10,
      y: 21,
      w: 101,
      h: 50,
    });
    expect(toDevice({ left: 10, top: 20, width: 100, height: 50 }, 2)).toEqual({
      x: 20,
      y: 40,
      w: 200,
      h: 100,
    });
    expect(toDevice({ left: 1, top: 1, width: 1, height: 1 }, Number.NaN)).toEqual({
      x: 1,
      y: 1,
      w: 1,
      h: 1,
    });
  });

  it('clips a slot half above the pane and keeps its full slot', () => {
    const p = placement({ left: 100, top: 0, width: 200, height: 100 }, pane, 1);
    expect(p.slot).toEqual({ x: 100, y: 0, w: 200, h: 100 });
    expect(p.clip).toEqual({ x: 100, y: 40, w: 200, h: 60 });
    expect(p.visible).toBe(true);
  });

  it('hides a slot outside the pane', () => {
    const p = placement({ left: 100, top: 600, width: 200, height: 100 }, pane, 1);
    expect(p.visible).toBe(false);
    expect(p.clip.w * p.clip.h).toBe(0);
  });

  it('intersects as the Rust Placement does', () => {
    expect(intersect({ x: 0, y: -10, w: 100, h: 50 }, { x: 0, y: 0, w: 800, h: 600 })).toEqual({
      x: 0,
      y: 0,
      w: 100,
      h: 40,
    });
  });

  it('treats a slot near the pane as near and ranks by distance to the centre', () => {
    const above = { left: 0, top: -150, width: 100, height: 100 };
    expect(nearPane(above, pane, 200)).toBe(true);
    expect(nearPane(above, pane, 0)).toBe(false);
    const mid = { left: 0, top: 240, width: 100, height: 100 };
    expect(centreDistance(mid, pane)).toBe(0);
    expect(centreDistance(above, pane)).toBeGreaterThan(centreDistance(mid, pane));
  });
});
