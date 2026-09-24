import { describe, expect, it, vi } from 'vitest';
import { cssScaleFor, renderGeometry } from '../lib/zoom';
import { commitTextLayer } from './useBitmapWindow';
import type { PdfTextContent, PdfViewport } from '../lib/pdfjs';
import type { TextLayerCtor } from './usePdfDocument';

const A4 = { width: 595, height: 842 };
const CSS_WIDTH = 800;

function fakeLayer() {
  const props = new Map<string, string>();
  const el = {
    replaceChildren: vi.fn(),
    style: {
      setProperty: vi.fn((k: string, v: string) => {
        props.set(k, v);
      }),
      getPropertyValue: (k: string) => props.get(k) ?? '',
    },
  };
  return { el, props };
}

function fakeCtor() {
  const seen: {
    textContentSource: PdfTextContent;
    container: HTMLElement;
    viewport: PdfViewport;
  }[] = [];
  const render = vi.fn(() => Promise.resolve());
  class FakeTextLayer {
    constructor(args: {
      textContentSource: PdfTextContent;
      container: HTMLElement;
      viewport: PdfViewport;
    }) {
      seen.push(args);
    }
    render = render;
  }
  return { ctor: FakeTextLayer as unknown as TextLayerCtor, seen, render };
}

describe('text layer viewport scale', () => {
  it('uses CSS px per pt at 1x DPR', () => {
    const text = cssScaleFor(CSS_WIDTH, A4);
    expect(text).toBeCloseTo(CSS_WIDTH / A4.width, 10);
    expect(renderGeometry(CSS_WIDTH, A4, 1).scale).toBeCloseTo(text, 10);

    const { el, props } = fakeLayer();
    const { ctor, seen, render } = fakeCtor();
    const textContent = {} as PdfTextContent;
    const viewport = { scale: text } as PdfViewport;
    commitTextLayer(el as unknown as HTMLElement, ctor, textContent, viewport);

    expect(seen).toHaveLength(1);
    expect(seen[0].container).toBe(el);
    expect(seen[0].textContentSource).toBe(textContent);
    expect(seen[0].viewport.scale).toBeCloseTo(CSS_WIDTH / A4.width, 10);
    expect(props.get('--total-scale-factor')).toBe(String(text));
    expect(el.replaceChildren).toHaveBeenCalledTimes(1);
    expect(render).toHaveBeenCalledTimes(1);
  });

  it('stays CSS px per pt at 2x DPR while the canvas backing folds DPR', () => {
    const folded = renderGeometry(CSS_WIDTH, A4, 2).scale;
    expect(folded).toBeCloseTo((2 * CSS_WIDTH) / A4.width, 10);
    const text = cssScaleFor(CSS_WIDTH, A4);
    expect(text).toBeCloseTo(CSS_WIDTH / A4.width, 10);

    const { el, props } = fakeLayer();
    const { ctor, seen } = fakeCtor();
    const viewport = { scale: text } as PdfViewport;
    commitTextLayer(el as unknown as HTMLElement, ctor, {} as PdfTextContent, viewport);

    expect(seen[0].viewport.scale).toBeCloseTo(text, 10);
    expect(seen[0].viewport.scale).not.toBeCloseTo(folded, 2);
    expect(props.get('--total-scale-factor')).toBe(String(text));
  });

  it('never blocks paint when the text layer throws', () => {
    const { el } = fakeLayer();
    class BrokenTextLayer {
      constructor() {
        throw new Error('no spans here');
      }
    }
    expect(() =>
      commitTextLayer(
        el as unknown as HTMLElement,
        BrokenTextLayer as unknown as TextLayerCtor,
        {} as PdfTextContent,
        { scale: 1 } as PdfViewport,
      ),
    ).not.toThrow();
  });
});
