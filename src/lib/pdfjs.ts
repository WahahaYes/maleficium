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