import { Typography } from '@mui/material';
import { useEffect, useRef, useState } from 'react';
import { openPdf, openPdfFromBytes } from '../lib/pdfjs';
import { readFile } from '@tauri-apps/plugin-fs';

interface PreviewProps {
  pdfUrl: string | null;
}

export default function Preview({ pdfUrl }: PreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [numPages, setNumPages] = useState(1);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!pdfUrl || !canvasRef.current) return;

    let isCancelled = false;

    const loadImage = async () => {
      try {
        setError(null);
        const pdf = pdfUrl.startsWith('blob:') || pdfUrl.startsWith('http://') || pdfUrl.startsWith('https://') || pdfUrl.startsWith('asset:')
          ? await openPdf(pdfUrl)
          : await openPdfFromBytes(new Uint8Array(await readFile(pdfUrl)));
        
        if (isCancelled) return;
        setNumPages(pdf.numPages);
        const page = await pdf.getPage(1);
        const viewport = page.getViewport({ scale: 1.2 });
        canvasRef.current!.height = viewport.height;
        canvasRef.current!.width = viewport.width;
        await page.render({ canvas: canvasRef.current, viewport }).promise;
      } catch (e) {
        if (!isCancelled) {
          setError(`Failed to load PDF: ${String(e)}`);
        }
      }
    };
    loadImage();

    return () => {
      isCancelled = true;
    };
  }, [pdfUrl]);

  if (!pdfUrl) return <Typography variant="body1">No PDF yet</Typography>;

  if (error) return <Typography variant="body1" color="error">{error}</Typography>;

  return (
    <>
      <Typography variant="subtitle2">Page 1 of {numPages}</Typography>
      <canvas ref={canvasRef} />
    </>
  );
}