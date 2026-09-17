// BinaryPreview.tsx — rich preview for non-text files.
//
// Text stays in the editor; everything else lands here: images render,
// video plays (native controls), PDFs reuse the continuous Preview,
// anything unknown gets an honest binary notice (size + kind + warning).
// Object URLs are revoked on path change/unmount (scale law: bytes stay at
// the edge, never in shared state).

import { useEffect, useState } from 'react';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import { loadPreviewBytes, previewKindFor, extOf, mimeFor } from '../lib/files';
import Preview from './Preview';

export default function BinaryPreview({ path }: { path: string }) {
  const kind = previewKindFor(path);
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (kind === 'pdf') return;
    let live = true;
    let obj: string | null = null;
    setUrl(null);
    setError(null);
    void (async () => {
      try {
        const bytes = await loadPreviewBytes(path);
        if (!live) return;
        const blob = new Blob([new Uint8Array(bytes)], { type: mimeFor(path) });
        obj = URL.createObjectURL(blob);
        if (live) setUrl(obj);
        else URL.revokeObjectURL(obj);
      } catch (e) {
        if (live) setError(`Preview unavailable: ${String(e).slice(0, 120)}`);
      }
    })();
    return () => {
      live = false;
      if (obj) URL.revokeObjectURL(obj);
    };
  }, [path, kind]);

  if (kind === 'pdf') {
    // Source PDFs (figures, references) render through the same scroller as
    // the compiled output: no separate viewer to learn or maintain.
    return <Preview pdfUrl={path} stamp={0} />;
  }

  if (error) return <Typography variant="body2" color="error">{error}</Typography>;
  if (!url) return <Typography variant="body2" color="text.secondary">Loading preview…</Typography>;

  if (kind === 'image') {
    return (
      <Box sx={{ flex: 1, overflow: 'auto', display: 'flex', alignItems: 'flex-start', justifyContent: 'center', p: 1 }}>
        <Box component="img" src={url} alt={path} sx={{ maxWidth: '100%' }} />
      </Box>
    );
  }

  if (kind === 'video') {
    return (
      <Box sx={{ flex: 1, overflow: 'auto', display: 'flex', alignItems: 'flex-start', justifyContent: 'center', p: 1 }}>
        <Box component="video" src={url} controls sx={{ maxWidth: '100%' }} />
      </Box>
    );
  }

  // Fallback: unknown binary. Honest, never raw bytes in the editor.
  return (
    <Box sx={{ p: 2 }}>
      <Typography variant="body2">Binary file — no preview for this type ({extOf(path) || 'unknown'}).</Typography>
      <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 1 }}>
        Open externally to view. Path: {path}
      </Typography>
    </Box>
  );
}
