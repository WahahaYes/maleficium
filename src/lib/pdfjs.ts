// pdfjs.ts — single pdf.js entry: worker setup + open helpers.
//
// Owns the pdf.js import and exposes the TextLayer constructor for the
// selectable-text overlay. Canvas is paint, DOM spans are the document.
// Filesystem knowledge (blob:/http:/asset: vs Tauri readFile) lives here.
import 'pdfjs-dist/web/pdf_viewer.css';
import { fs } from './fs-provider';
import type {
  PDFDocumentProxy,
  PDFPageProxy,
  TextLayer,
  PageViewport,
} from 'pdfjs-dist/types/src/pdf';
import type { TextContent } from 'pdfjs-dist/types/src/display/api';

type PdfJsModule = typeof import('pdfjs-dist/types/src/pdf');

let cached: { pdfJs: PdfJsModule; TextLayer: typeof TextLayer } | null = null;

export async function getPdfJs(): Promise<{
  pdfJs: PdfJsModule;
  TextLayer: typeof TextLayer;
}> {
  if (cached) return cached;
  const pdfJs = (await import('pdfjs-dist/legacy/build/pdf.mjs')) as unknown as PdfJsModule;
  const worker = new Worker(new URL('pdfjs-dist/legacy/build/pdf.worker.mjs', import.meta.url), {
    type: 'module',
  });
  pdfJs.GlobalWorkerOptions.workerPort = worker;
  cached = { pdfJs, TextLayer: pdfJs.TextLayer };
  return cached;
}

export type PdfPage = PDFPageProxy;
export type PdfDoc = PDFDocumentProxy;
export type PdfTextContent = TextContent;
export type PdfViewport = PageViewport;

export async function openPdf(url: string): Promise<PDFDocumentProxy> {
  const { pdfJs } = await getPdfJs();
  return await pdfJs.getDocument(url).promise;
}

export async function openPdfFromBytes(data: Uint8Array): Promise<PDFDocumentProxy> {
  const { pdfJs } = await getPdfJs();
  return await pdfJs.getDocument({ data }).promise;
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
