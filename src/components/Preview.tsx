import { Typography } from '@mui/material';
import { useEffect, useRef } from 'react';

interface PreviewProps {
  pdfUrl: string | null;
}

export default function Preview({ pdfUrl }: PreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (!pdfUrl || !canvasRef.current) return;

    const loadImage = async () => {
      const pdfJs = await import('pdfjs-dist');
      const workerUrl = new URL('pdfjs-dist/build/pdf.worker.min.mjs?url', import.meta.url).toString();
      pdfJs.GlobalWorkerOptions.workerSrc = workerUrl;
      const pdf = await pdfJs.getDocument(pdfUrl).promise;
      const page = await pdf.getPage(1);
      const viewport = page.getViewport({ scale: 1.2 });
      canvasRef.current!.height = viewport.height;
      canvasRef.current!.width = viewport.width;
      await page.render({ canvas: canvasRef.current, viewport }).promise;
    };
    loadImage();
  }, [pdfUrl]);

  if (!pdfUrl) return <Typography variant="body1">No PDF yet</Typography>;

  return (
    <>
      <Typography variant="subtitle2">Page 1 of 1</Typography>
      <canvas ref={canvasRef} />
    </>
  );
}