// pdfjs.ts — single pdf.js entry: worker setup + TextLayer handoff.
//
// Preview never imports pdf.js directly: this module owns the import (one
// renderer, not two) and hands Preview the TextLayer constructor for the
// selectable-text overlay. Canvas is paint, DOM spans are the document.
import 'pdfjs-dist/web/pdf_viewer.css';

let cachedPdfJs: any = null;

async function getPdfJs(): Promise<any> {
  if (cachedPdfJs) return cachedPdfJs;
  const pdfJs = await import('pdfjs-dist/legacy/build/pdf.mjs');
  const { GlobalWorkerOptions } = pdfJs as any;
  const worker = new Worker(
    new URL('pdfjs-dist/legacy/build/pdf.worker.mjs', import.meta.url),
    { type: 'module' }
  );
  GlobalWorkerOptions.workerPort = worker;
  cachedPdfJs = pdfJs;
  // Hand the TextLayer constructor to Preview (single path — Preview never
  // imports pdf.js directly, so the viewer stays one renderer, not two).
  try {
    const { registerTextLayer } = await import('../components/Preview');
    registerTextLayer(pdfJs.TextLayer ?? null);
  } catch {
    /* headless/test env — canvas paint still commits without text */
  }
  return cachedPdfJs;
}

export async function openPdf(url: string): Promise<any> {
  const pdfJs = await getPdfJs();
  return await pdfJs.getDocument(url).promise;
}

export async function openPdfFromBytes(data: Uint8Array): Promise<any> {
  const pdfJs = await getPdfJs();
  return await pdfJs.getDocument({ data }).promise;
}