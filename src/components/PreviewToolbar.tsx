// PreviewToolbar.tsx — toolbar pager (prev/page/total/next + sync + popout).
//
// Growth cap: constant height; pagination (NOT infinite scroll) is the memory
// contract — 1 visible page + LRU 5-page cache in Preview body.

import Toolbar from '@mui/material/Toolbar';
import ButtonGroup from '@mui/material/ButtonGroup';
import Button from '@mui/material/Button';
import IconButton from '@mui/material/IconButton';
import TextField from '@mui/material/TextField';
import { useState } from 'react';

export default function PreviewToolbar({ pageNumber, totalPages, onPage, onSync, onPopout, compiling, syncDisabled }: {
  pageNumber: number;
  totalPages: number;
  onPage: (p: number) => void;
  onSync: () => void;
  onPopout: () => void;
  compiling?: boolean;
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
        <Button aria-label="Previous page" onClick={() => onPage(Math.max(1, pageNumber - 1))} disabled={pageNumber <= 1}>{'<'}</Button>
        <Button disabled aria-label={`Page ${pageNumber} of ${totalPages}`}>{pageNumber} / {totalPages}</Button>
        <Button aria-label="Next page" onClick={() => onPage(Math.min(totalPages, pageNumber + 1))} disabled={pageNumber >= totalPages}>{'>'}</Button>
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
      <IconButton size="small" aria-label="SyncTeX to cursor" title="SyncTeX to cursor" disabled={!!compiling || !!syncDisabled} onClick={onSync}>◎</IconButton>
      <IconButton size="small" aria-label="Popout preview (stub)" title="Popout preview (stub)" onClick={onPopout}>↗</IconButton>
    </Toolbar>
  );
}
