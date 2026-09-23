// PreviewToolbar.tsx — toolbar pager (prev/jump/total/next + sync) and zoom.
//
// Wraps onto a second row in a narrow pane rather than clipping controls.
// Pager jumps drive the preview target; the preview scrolls
// after the target bitmap lands (never onto blank shells).

import Toolbar from '@mui/material/Toolbar';
import ButtonGroup from '@mui/material/ButtonGroup';
import Button from '@mui/material/Button';
import IconButton from '@mui/material/IconButton';
import TextField from '@mui/material/TextField';
import { useState } from 'react';
import NavigateBeforeIcon from '@mui/icons-material/NavigateBefore';
import NavigateNextIcon from '@mui/icons-material/NavigateNext';
import SyncIcon from '@mui/icons-material/Sync';
import ZoomInIcon from '@mui/icons-material/ZoomIn';
import ZoomOutIcon from '@mui/icons-material/ZoomOut';
import Menu from '@mui/material/Menu';
import MenuItem from '@mui/material/MenuItem';
import type { ZoomAction, ZoomMode } from '../lib/zoom';

/** Fixed choices in the zoom menu, beside the two fit modes. */
const ZOOM_PRESETS = [50, 75, 100, 125, 150, 200];

export default function PreviewToolbar({
  pageNumber,
  totalPages,
  onPage,
  onSync,
  syncDisabled,
  zoomLabel,
  onZoom,
  onZoomMode,
}: {
  pageNumber: number;
  totalPages: number;
  onPage: (p: number) => void;
  onSync: () => void;
  syncDisabled?: boolean;
  zoomLabel: string;
  onZoom: (a: ZoomAction) => void;
  onZoomMode: (m: ZoomMode) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const [zoomMenu, setZoomMenu] = useState<HTMLElement | null>(null);
  const pick = (m: ZoomMode) => {
    setZoomMenu(null);
    onZoomMode(m);
  };
  const shown = draft ?? String(pageNumber);
  const commitDraft = () => {
    if (draft == null) return;
    const n = parseInt(draft, 10);
    setDraft(null);
    if (Number.isFinite(n)) onPage(Math.min(totalPages, Math.max(1, n)));
  };
  return (
    <Toolbar
      disableGutters
      variant="dense"
      sx={{ gap: 1, rowGap: 0.5, minHeight: 40, flexWrap: 'wrap', py: 0.5 }}
    >
      <ButtonGroup size="small">
        <Button
          aria-label="Previous page"
          onClick={() => onPage(Math.max(1, pageNumber - 1))}
          disabled={pageNumber <= 1}
        >
          <NavigateBeforeIcon fontSize="small" />
        </Button>
        <Button disabled aria-label={`Page ${pageNumber} of ${totalPages}`}>
          {pageNumber} / {totalPages}
        </Button>
        <Button
          aria-label="Next page"
          onClick={() => onPage(Math.min(totalPages, pageNumber + 1))}
          disabled={pageNumber >= totalPages}
        >
          <NavigateNextIcon fontSize="small" />
        </Button>
      </ButtonGroup>
      <TextField
        size="small"
        aria-label="Go to page"
        value={shown}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commitDraft}
        onKeyDown={(e) => {
          if (e.key === 'Enter') commitDraft();
        }}
        sx={{ width: 72 }}
        slotProps={{ htmlInput: { inputMode: 'numeric' } }}
      />
      <IconButton
        size="small"
        aria-label="SyncTeX to cursor"
        title="SyncTeX to cursor"
        disabled={!!syncDisabled}
        onClick={onSync}
      >
        <SyncIcon fontSize="small" />
      </IconButton>
      <ButtonGroup size="small" sx={{ ml: 'auto' }}>
        <Button aria-label="Zoom out" title="Zoom out (Ctrl+-)" onClick={() => onZoom('out')}>
          <ZoomOutIcon fontSize="small" />
        </Button>
        <Button
          aria-label="Zoom"
          title="Zoom"
          onClick={(e) => setZoomMenu(e.currentTarget)}
          sx={{ textTransform: 'none', minWidth: 72 }}
        >
          {zoomLabel}
        </Button>
        <Button aria-label="Zoom in" title="Zoom in (Ctrl+=)" onClick={() => onZoom('in')}>
          <ZoomInIcon fontSize="small" />
        </Button>
      </ButtonGroup>
      <Menu anchorEl={zoomMenu} open={zoomMenu != null} onClose={() => setZoomMenu(null)}>
        <MenuItem onClick={() => pick({ kind: 'fit-width' })}>Fit width</MenuItem>
        <MenuItem onClick={() => pick({ kind: 'fit-page' })}>Fit page</MenuItem>
        {ZOOM_PRESETS.map((p) => (
          <MenuItem key={p} onClick={() => pick({ kind: 'percent', percent: p })}>
            {p}%
          </MenuItem>
        ))}
      </Menu>
    </Toolbar>
  );
}
