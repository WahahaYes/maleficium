import { Typography } from '@mui/material';
import { useEffect, useRef, useState } from 'react';
import { openPdf } from '../lib/pdfjs';

interface PreviewProps {
  pdfUrl: string | null;
}

export default function Preview({ pdfUrl }: PreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [numPages, setNumPages] = useState(1);

  useEffect(() => {
    if (!pdfUrl || !canvasRef.current) return;

    const loadImage = async () => {
      const pdf = await openPdf(pdfUrl);
      setNumPages(pdf.numPages);
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
      <Typography variant="subtitle2">Page 1 of {numPages}</Typography>
      <canvas ref={canvasRef} />
    </>
  );
}