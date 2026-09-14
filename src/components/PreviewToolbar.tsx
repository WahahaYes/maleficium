// PreviewToolbar.tsx — toolbar pager (prev/jump/total/next + sync).
//
// Growth cap: constant height; the Preview body owns the scroll contract
// (all shells mounted, windowed bitmaps). Pager jumps drive the body target;
// the body scrolls AFTER the target bitmap lands (never onto blank shells).

import Toolbar from '@mui/material/Toolbar';
import ButtonGroup from '@mui/material/ButtonGroup';
import Button from '@mui/material/Button';
import IconButton from '@mui/material/IconButton';
import TextField from '@mui/material/TextField';
import { useState } from 'react';
import NavigateBeforeIcon from '@mui/icons-material/NavigateBefore';
import NavigateNextIcon from '@mui/icons-material/NavigateNext';
import SyncIcon from '@mui/icons-material/Sync';

export default function PreviewToolbar({ pageNumber, totalPages, onPage, onSync, syncDisabled }: {
  pageNumber: number;
  totalPages: number;
  onPage: (p: number) => void;
  onSync: () => void;
  syncDisabled?: boolean;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const shown = draft ?? String(pageNumber);
  const commitDraft = () => {
    if (draft == null) return;
    const n = parseInt(draft, 10);
    setDraft(null);
    if (Number.isFinite(n)) onPage(Math.min(totalPages, Math.max(1, n)));
  };
  return (
    <Toolbar disableGutters variant="dense" sx={{ gap: 1, minHeight: 40 }}>
      <ButtonGroup size="small">
        <Button aria-label="Previous page" onClick={() => onPage(Math.max(1, pageNumber - 1))} disabled={pageNumber <= 1}><NavigateBeforeIcon fontSize="small" /></Button>
        <Button disabled aria-label={`Page ${pageNumber} of ${totalPages}`}>{pageNumber} / {totalPages}</Button>
        <Button aria-label="Next page" onClick={() => onPage(Math.min(totalPages, pageNumber + 1))} disabled={pageNumber >= totalPages}><NavigateNextIcon fontSize="small" /></Button>
      </ButtonGroup>
      <TextField
        size="small"
        aria-label="Go to page"
        value={shown}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commitDraft}
        onKeyDown={(e) => { if (e.key === 'Enter') commitDraft(); }}
        sx={{ width: 72 }}
        slotProps={{ htmlInput: { inputMode: 'numeric' } }}
      />
      <IconButton size="small" aria-label="SyncTeX to cursor" title="SyncTeX to cursor" disabled={!!syncDisabled} onClick={onSync}><SyncIcon fontSize="small" /></IconButton>
    </Toolbar>
  );
}
