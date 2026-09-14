import { describe, it, expect } from 'vitest';
import { windowFor, pickVisible, compensateScrollTop, clampPage } from './previewNav';

describe('preview window shape', () => {
  it('centers the bitmap window on the anchor', () => {
    expect(windowFor(5, 50)).toEqual({ lo: 4, hi: 7, pages: [4, 5, 6, 7] });
  });
  it('clamps the window at document edges', () => {
    expect(windowFor(1, 50)).toEqual({ lo: 1, hi: 3, pages: [1, 2, 3] });
    expect(windowFor(50, 50)).toEqual({ lo: 49, hi: 50, pages: [49, 50] });
  });
  it('collapses to a single page document', () => {
    expect(windowFor(1, 1)).toEqual({ lo: 1, hi: 1, pages: [1] });
  });
  it('clamps out-of-range anchors into the document', () => {
    expect(clampPage(0, 50)).toBe(1);
    expect(clampPage(99, 50)).toBe(50);
    expect(windowFor(99, 50).hi).toBe(50);
  });
});

describe('visible page pick', () => {
  it('picks the largest intersection ratio', () => {
    expect(pickVisible([
      { page: 1, ratio: 0.2 },
      { page: 2, ratio: 0.8 },
      { page: 3, ratio: 0.4 },
    ])).toBe(2);
  });
  it('breaks ties toward the smaller page so page 1 stays reachable', () => {
    expect(pickVisible([
      { page: 2, ratio: 0.5 },
      { page: 1, ratio: 0.5 },
    ])).toBe(1);
  });
  it('keeps the current page when nothing intersects', () => {
    expect(pickVisible([])).toBeNull();
    expect(pickVisible([{ page: 3, ratio: 0 }])).toBeNull();
  });
});

describe('scroll compensation', () => {
  it('holds the viewport stable when content mounts above', () => {
    expect(compensateScrollTop(400, 200)).toBe(600);
  });
  it('never pulls the viewport up', () => {
    expect(compensateScrollTop(400, 0)).toBe(400);
    expect(compensateScrollTop(400, -50)).toBe(400);
  });
});
