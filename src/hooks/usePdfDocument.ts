// Document lifecycle for the windowed PDF preview.
//
// Opens the pdf.js document ONCE per pdfUrl+stamp (renders serve from the
// cached handle), probes page aspects so shells reserve true heights, and
// resets the shared render identity on every revision. Per-task liveness:
// a superseding document cancels the probe via docKey match; the load's own
// unmount cancels via a local flag.
// Emits `pdf loaded N pages in Xms`.

import { useCallback, useEffect, useState, type RefObject } from 'react';
import { getPdfJs, openPdfSource } from '../lib/pdfjs';
import type { PdfTextContent, PdfViewport } from '../lib/pdfjs';
import type { PDFDocumentProxy, PDFPageProxy } from 'pdfjs-dist/types/src/pdf';
import { emit } from '../lib/events';
import { clampPage } from '../lib/previewNav';
import { JUMP_LOCK_MS } from './useSyncLock';

export interface LocalPdfPage {
  getViewport: (o: { scale: number }) => PdfViewport;
  render: (o: {
    canvasContext: CanvasRenderingContext2D;
    viewport: PdfViewport;
    canvas?: HTMLCanvasElement | null;
  }) => {
    promise: Promise<void>;
  };
  getTextContent: () => Promise<PdfTextContent>;
}

export type PageLike = PDFPageProxy | LocalPdfPage;
export type DocLike =
  PDFDocumentProxy | { numPages: number; getPage: (n: number) => Promise<PageLike> };

export interface PdfJsApi {
  TextLayer: new (o: {
    textContentSource: PdfTextContent;
    container: HTMLElement;
    viewport: PdfViewport;
  }) => { render: () => Promise<void> };
}

export type TextLayerCtor = PdfJsApi['TextLayer'];

export interface PdfDocumentParams {
  pdfUrl: string | null;
  stamp: number;
  scrollRef: RefObject<HTMLDivElement | null>;
  shellRefs: RefObject<Map<number, HTMLDivElement>>;
  docRef: RefObject<{ key: string; pdf: DocLike } | null>;
  textLayerRef: RefObject<TextLayerCtor | null>;
  dimsRef: RefObject<Map<number, { w: number; h: number }>>;
  compRef: RefObject<{ page: number; delta: number } | null>;
  renderedRef: RefObject<Set<string>>;
  renderedKeys: RefObject<Map<number, string>>;
  inFlightRef: RefObject<Map<string, number>>;
  ratiosRef: RefObject<Map<number, number>>;
  visibleRef: RefObject<number>;
  pageRef: RefObject<number>;
  pendingScrollRef: RefObject<number | null>;
  lockRef: RefObject<{ target: number; until: number } | null>;
  setVisible: (n: number) => void;
  setPhase: (s: string) => void;
  setNumPages: (n: number) => void;
  setDocKey: (k: string) => void;
  bumpDims: () => void;
  armJumpTimeout: (target: number) => void;
}

export interface PdfDocument {
  /** Load failure message, or null while the document is healthy. */
  error: string | null;
}

export function usePdfDocument({
  pdfUrl,
  stamp,
  scrollRef,
  shellRefs,
  docRef,
  textLayerRef,
  dimsRef,
  compRef,
  renderedRef,
  renderedKeys,
  inFlightRef,
  ratiosRef,
  visibleRef,
  pageRef,
  pendingScrollRef,
  lockRef,
  setVisible,
  setPhase,
  setNumPages,
  setDocKey,
  bumpDims,
  armJumpTimeout,
}: PdfDocumentParams): PdfDocument {
  const [error, setError] = useState<string | null>(null);

  // Probe page aspects (scale-1 viewports, no pixels) so shells reserve true
  // heights before bitmaps land. Page 1 publishes immediately with its aspect
  // assumed for the whole document (uniform docs settle in one layout pass);
  // later batches publish ONLY when a page differs from the assumption, with
  // an anchor snapshot each time so the viewport can be re-anchored below.
  // Cancellation is per-DOCUMENT (docKey match): a newer document supersedes
  // the probe, but nothing else does — there is no shared generation for a
  // re-render or a neighbor pass to trip over.
  const probeDims = useCallback(
    async (pdf: DocLike, total: number, alive: () => boolean) => {
      const snapAnchor = () => {
        const box = scrollRef.current;
        const anchorPage = visibleRef.current;
        const shell = shellRefs.current.get(anchorPage);
        compRef.current =
          box && shell
            ? {
                page: anchorPage,
                delta: shell.getBoundingClientRect().top - box.getBoundingClientRect().top,
              }
            : null;
      };
      const differs = (a: { w: number; h: number }, b: { w: number; h: number }) =>
        Math.abs(a.w - b.w) / Math.max(1, b.w) > 0.005 ||
        Math.abs(a.h - b.h) / Math.max(1, b.h) > 0.005;
      try {
        const first = await pdf.getPage(1);
        if (!alive()) return;
        const v1 = first.getViewport({ scale: 1 });
        const base = { w: v1.width, h: v1.height };
        const dims = new Map<number, { w: number; h: number }>();
        for (let n = 1; n <= total; n++) dims.set(n, base);
        dimsRef.current = dims;
        snapAnchor();
        bumpDims();
        for (let n = 2; n <= total; n++) {
          if (!alive()) return;
          const pg = await pdf.getPage(n);
          if (!alive()) return;
          const v = pg.getViewport({ scale: 1 });
          const next = { w: v.width, h: v.height };
          if (!differs(next, dims.get(n) as { w: number; h: number })) continue;
          dims.set(n, next);
          if (n % 10 === 0 || n === total) {
            if (!alive()) return;
            dimsRef.current = new Map(dims);
            snapAnchor();
            bumpDims();
            await new Promise<void>((r) => setTimeout(r, 0));
          }
        }
      } catch {
        // Probe never blocks the preview; shells keep the default aspect.
      }
    },
    [bumpDims, compRef, dimsRef, scrollRef, shellRefs, visibleRef],
  );

  // Document open (per pdfUrl+stamp) + canonical reset to the pager page.
  // Section-count state (numPages) commits together with the doc identity so
  // the shell list and the observer mount in the same pass.
  useEffect(() => {
    if (!pdfUrl) return;
    let cancelled = false;
    const yieldUi = () => new Promise<void>((r) => setTimeout(r, 0));

    const load = async () => {
      try {
        setError(null);
        try {
          if (!textLayerRef.current) {
            textLayerRef.current = ((await getPdfJs()).TextLayer ?? null) as TextLayerCtor | null;
          }
        } catch {
          /* no text layer here — canvas paint still commits */
        }
        const key = `${pdfUrl}#${stamp}`;
        let pdf: DocLike;
        if (docRef.current?.key === key) {
          setPhase('rendering...');
          pdf = docRef.current.pdf;
        } else {
          setPhase('reading...');
          const t0 = Date.now();
          await yieldUi();
          setPhase('loading...');
          pdf = await openPdfSource(pdfUrl);
          if (cancelled) return;
          const ms = Date.now() - t0;
          emit({
            scope: 'preview',
            kind: 'progress',
            actor: 'system',
            message: `pdf loaded ${pdf.numPages} pages in ${ms}ms`,
            event: { action: 'preview.pdf-load', pages: pdf.numPages, ms },
          });
          docRef.current = { key, pdf };
        }
        if (cancelled) return;
        await yieldUi();
        if (cancelled) return;
        // Fresh identity per revision: bitmaps, ratios, and aspects restart.
        renderedRef.current.clear();
        renderedKeys.current.clear();
        inFlightRef.current.clear();
        ratiosRef.current.clear();
        dimsRef.current = new Map();
        bumpDims();
        const target = clampPage(pageRef.current, pdf.numPages);
        visibleRef.current = target;
        setVisible(target);
        pendingScrollRef.current = target;
        lockRef.current = { target, until: Date.now() + JUMP_LOCK_MS };
        armJumpTimeout(target);
        setPhase('');
        if (!cancelled) {
          setNumPages(pdf.numPages);
          setDocKey(key);
        }
        // Probe liveness is per-document: superseded only when a NEWER docKey
        // commits (docRef moves on), never by renders or re-mounts.
        void probeDims(pdf, pdf.numPages, () => !cancelled && docRef.current?.key === key);
      } catch (e) {
        if (!cancelled) {
          setError(`Failed to load PDF: ${String(e)}`);
          setPhase('');
        }
      }
    };
    void load();

    return () => {
      cancelled = true;
    };
  }, [
    pdfUrl,
    stamp,
    probeDims,
    armJumpTimeout,
    bumpDims,
    dimsRef,
    docRef,
    inFlightRef,
    lockRef,
    pageRef,
    pendingScrollRef,
    ratiosRef,
    renderedKeys,
    renderedRef,
    setDocKey,
    setNumPages,
    setPhase,
    setVisible,
    textLayerRef,
    visibleRef,
  ]);

  return { error };
}
