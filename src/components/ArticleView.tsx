// ArticleView.tsx — the Article tab body.
//
// The bundle bytes render in a sandboxed frame: sandbox="allow-scripts"
// exactly (the reader e2e probes pin this shape), opaque origin, no bridge
// inside. The bytes keep the reader's own CSP meta. An unapproved widget
// shows its poster with an Approve button from the bytes themselves; the
// button only asks the app (the Widgets panel opens so the user reviews
// and approves there, then the article re-exports and re-renders).
// Editor-to-article sync is forward-only: the caret's anchor posts into the
// frame and the bytes scroll to it; the article never reports back.

import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Typography from '@mui/material/Typography';
import { useEffect, useRef } from 'react';
import { articleSandbox, articleScrollMessage, isArticleMessage } from '../lib/article';

export interface ArticleViewProps {
  /** The bundle bytes, or null before the first load. */
  html: string | null;
  loading: boolean;
  error: string | null;
  /** False with no compiled output to read (nothing to load yet). */
  canLoad: boolean;
  /** True while a compile runs (the bytes would describe the old document). */
  compiling: boolean;
  zoomPercent: number;
  onLoad: () => void;
  /** The caret section's anchor, or null when the caret names none. */
  anchorId: string | null;
  /** Fires when the caret section is posted into the frame. */
  onSync?: (anchorId: string) => void;
  /** Fires when the bytes' Approve button asks for a widget. */
  onApproveRequest?: (widgetId: string) => void;
}

export default function ArticleView({
  html,
  loading,
  error,
  canLoad,
  compiling,
  zoomPercent,
  onLoad,
  anchorId,
  onSync,
  onApproveRequest,
}: ArticleViewProps) {
  const frameRef = useRef<HTMLIFrameElement>(null);
  const syncRef = useRef(onSync);
  syncRef.current = onSync;
  const approveRef = useRef(onApproveRequest);
  approveRef.current = onApproveRequest;

  // Denial point for frame messages. Validates origin, source and shape,
  // then hands the approval ask to the Widgets panel round-trip.
  useEffect(() => {
    if (!html) return;
    const onMessage = (e: MessageEvent) => {
      if (!isArticleMessage(e.data, e.origin, e.source, frameRef.current?.contentWindow)) return;
      approveRef.current?.(e.data.widgetId);
    };
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  }, [html]);

  // Forward-only sync: post the caret's anchor into the frame, where the
  // bytes scroll to it. The post repeats twice: a fresh srcDoc frame may
  // not have its listener yet on the first one. The article never replies.
  useEffect(() => {
    if (!html || !anchorId) return;
    const msg = articleScrollMessage(anchorId);
    const post = () => frameRef.current?.contentWindow?.postMessage(msg, '*');
    post();
    const t = setInterval(post, 250);
    const stop = setTimeout(() => clearInterval(t), 500);
    syncRef.current?.(anchorId);
    return () => {
      clearInterval(t);
      clearTimeout(stop);
    };
  }, [html, anchorId]);

  if (loading) return <Typography variant="body1">Loading article…</Typography>;

  if (error)
    return (
      <Box sx={{ display: 'flex', flexDirection: 'column', gap: 1, p: 2 }}>
        <Typography variant="body1" color="error">
          {error}
        </Typography>
        <Box>
          <Button size="small" onClick={onLoad} disabled={compiling}>
            Try again
          </Button>
        </Box>
      </Box>
    );

  if (!html) {
    if (compiling) return <Typography variant="body1">Compiling…</Typography>;
    if (!canLoad)
      return <Typography variant="body1">No article yet — compile the paper first</Typography>;
    return (
      <Box sx={{ display: 'flex', flexDirection: 'column', gap: 1, p: 2 }}>
        <Typography variant="body1">The article is not loaded</Typography>
        <Box>
          <Button size="small" onClick={onLoad}>
            Load article
          </Button>
        </Box>
      </Box>
    );
  }

  return (
    <Box sx={{ flex: 1, minHeight: 0, display: 'flex' }}>
      <iframe
        ref={frameRef}
        title="Article"
        sandbox={articleSandbox()}
        referrerPolicy="no-referrer"
        srcDoc={html}
        style={
          {
            width: '100%',
            height: '100%',
            border: 0,
            backgroundColor: 'transparent',
            zoom: zoomPercent / 100,
          } as React.CSSProperties
        }
      />
    </Box>
  );
}
