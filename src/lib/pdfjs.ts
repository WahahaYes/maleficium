// pdfjs.ts — single pdf.js entry: worker setup + open helpers.
//
// Owns the pdf.js import and exposes the TextLayer constructor for the
// selectable-text overlay. Canvas is paint, DOM spans are the document.
// Filesystem knowledge (blob:/http:/asset: vs Tauri readFile) lives here.
import 'pdfjs-dist/web/pdf_viewer.css';
import { emit } from './events';
import { fs } from './fs-provider';
import type {
  PDFDocumentProxy,
  PDFPageProxy,
  TextLayer,
  PageViewport,
} from 'pdfjs-dist/types/src/pdf';
import type { TextContent } from 'pdfjs-dist/types/src/display/api';

type PdfJsModule = typeof import('pdfjs-dist/types/src/pdf');

interface PdfJsHandle {
  pdfJs: PdfJsModule;
  TextLayer: typeof TextLayer;
  /** Rejects when this handle's worker fails to load. */
  workerFailed: Promise<never>;
}

let cached: PdfJsHandle | null = null;
let inflight: Promise<PdfJsHandle> | null = null;

// A module-worker load failure arrives as a bare 'error' Event with no
// 'ready' ever following, so an open would hang: arm a per-worker
// rejection every open races, reported once on the bus.
async function loadPdfJs(): Promise<PdfJsHandle> {
  const pdfJs = (await import('pdfjs-dist/legacy/build/pdf.mjs')) as unknown as PdfJsModule;
  const worker = new Worker(new URL('pdfjs-dist/legacy/build/pdf.worker.mjs', import.meta.url), {
    type: 'module',
  });
  let failWorker!: (e: Error) => void;
  const workerFailed: Promise<never> = new Promise<never>((_, reject) => {
    failWorker = reject;
  });
  // Unhandled until an open races it; settled races ignore it after.
  workerFailed.catch(() => {});
  worker.addEventListener(
    'error',
    (e) => {
      // A failed module load fires a bare Event; a runtime error
      // carries a message. Read the property, never the global.
      const message = (e as ErrorEvent).message;
      const detail =
        typeof message === 'string' && message ? message : 'pdf worker failed to load';
      if (cached?.workerFailed === workerFailed) cached = null;
      worker.terminate();
      emit({
        scope: 'preview',
        kind: 'error',
        actor: 'system',
        message: `pdf worker failed to load: ${detail}`.slice(0, 200),
        event: { action: 'preview.load-failed', error: detail.slice(0, 200) },
      });
      failWorker(new Error(`pdf worker failed to load: ${detail}`));
    },
    { once: true },
  );
  pdfJs.GlobalWorkerOptions.workerPort = worker;
  return { pdfJs, TextLayer: pdfJs.TextLayer, workerFailed };
}

export async function getPdfJs(): Promise<PdfJsHandle> {
  if (cached) return cached;
  inflight ??= loadPdfJs().then(
    (entry) => {
      cached = entry;
      inflight = null;
      return entry;
    },
    (e) => {
      inflight = null;
      throw e;
    },
  );
  return inflight;
}

export type PdfPage = PDFPageProxy;
export type PdfDoc = PDFDocumentProxy;
export type PdfTextContent = TextContent;
export type PdfViewport = PageViewport;

export async function openPdf(url: string): Promise<PDFDocumentProxy> {
  const { pdfJs, workerFailed } = await getPdfJs();
  return await Promise.race([pdfJs.getDocument(url).promise, workerFailed]);
}

export async function openPdfFromBytes(data: Uint8Array): Promise<PDFDocumentProxy> {
  const { pdfJs, workerFailed } = await getPdfJs();
  return await Promise.race([pdfJs.getDocument({ data }).promise, workerFailed]);
}

function isRemoteSource(source: string): boolean {
  return (
    source.startsWith('blob:') ||
    source.startsWith('http://') ||
    source.startsWith('https://') ||
    source.startsWith('asset:')
  );
}

/** Open a Preview source: remote/blob/asset URLs via pdf.js, local paths via Tauri fs. */
export async function openPdfSource(source: string): Promise<PDFDocumentProxy> {
  if (isRemoteSource(source)) return openPdf(source);
  const bytes = new Uint8Array(await fs().readBytes(source));
  return openPdfFromBytes(bytes);
}
