// pdfjs.ts — single pdf.js entry: worker setup + open helpers.
//
// Preview never imports pdf.js directly: this module owns the import (one
// renderer, not two) and hands Preview the TextLayer constructor for the
// selectable-text overlay via getPdfJs(). Canvas is paint, DOM spans are
// the document. Filesystem knowledge (blob:/http:/asset: vs Tauri readFile)
// lives here too, so components never branch on URL prefixes.
import 'pdfjs-dist/web/pdf_viewer.css';
import { readFile } from '@tauri-apps/plugin-fs';

let cached: { pdfJs: any; TextLayer: any } | null = null;

export async function getPdfJs(): Promise<{ pdfJs: any; TextLayer: any }> {
  if (cached) return cached;
  const pdfJs = await import('pdfjs-dist/legacy/build/pdf.mjs');
  const { GlobalWorkerOptions } = pdfJs as any;
  const worker = new Worker(
    new URL('pdfjs-dist/legacy/build/pdf.worker.mjs', import.meta.url),
    { type: 'module' }
  );
  GlobalWorkerOptions.workerPort = worker;
  cached = { pdfJs, TextLayer: (pdfJs as any).TextLayer ?? null };
  return cached;
}

export async function openPdf(url: string): Promise<any> {
  const { pdfJs } = await getPdfJs();
  return await pdfJs.getDocument(url).promise;
}

export async function openPdfFromBytes(data: Uint8Array): Promise<any> {
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
export async function openPdfSource(source: string): Promise<any> {
  if (isRemoteSource(source)) return openPdf(source);
  const bytes = new Uint8Array(await readFile(source));
  return openPdfFromBytes(bytes);
}
