// PreviewToolbar.tsx — toolbar pager (prev/page/total/next + sync + popout).
//
// Growth cap: constant height; pagination (NOT infinite scroll) is the memory
// contract — 1 visible page + LRU 5-page cache in Preview body.

import Toolbar from '@mui/material/Toolbar';
import ButtonGroup from '@mui/material/ButtonGroup';
import Button from '@mui/material/Button';
import IconButton from '@mui/material/IconButton';
import Typography from '@mui/material/Typography';

export default function PreviewToolbar({ pageNumber, totalPages, onPage, onSync, onPopout }: {
  pageNumber: number;
  totalPages: number;
  onPage: (p: number) => void;
  onSync: () => void;
  onPopout: () => void;
}) {
  return (
    <Toolbar disableGutters variant="dense" sx={{ gap: 1, minHeight: 40 }}>
      <ButtonGroup size="small">
        <Button onClick={() => onPage(Math.max(1, pageNumber - 1))} disabled={pageNumber <= 1}>{'<'}</Button>
        <Button disabled>{pageNumber} / {totalPages}</Button>
        <Button onClick={() => onPage(Math.min(totalPages, pageNumber + 1))} disabled={pageNumber >= totalPages}>{'>'}</Button>
      </ButtonGroup>
      <IconButton size="small" title="SyncTeX to cursor" onClick={onSync}>◎</IconButton>
      <IconButton size="small" title="Popout preview (stub)" onClick={onPopout}>↗</IconButton>
      <Typography variant="caption" sx={{ ml: 'auto' }}>pager</Typography>
    </Toolbar>
  );
}
