// HistoryDialog.tsx — per-file revision list with restore.
//
// Presentational only: rows, availability, and caps arrive already formatted.
// Restoring keeps a revision of what was on disk first, so it is undoable
// from this same list.

import Button from '@mui/material/Button';
import Dialog from '@mui/material/Dialog';
import DialogActions from '@mui/material/DialogActions';
import DialogContent from '@mui/material/DialogContent';
import DialogTitle from '@mui/material/DialogTitle';
import Table from '@mui/material/Table';
import TableBody from '@mui/material/TableBody';
import TableCell from '@mui/material/TableCell';
import TableRow from '@mui/material/TableRow';
import Typography from '@mui/material/Typography';
import {
  HISTORY_EMPTY,
  HISTORY_UNAVAILABLE,
  type HistoryAvailability,
  type RevisionRow,
} from '../lib/history.view';

export default function HistoryDialog({
  open,
  onClose,
  fileLabel,
  availability,
  rows,
  summary,
  notice,
  restoringRev,
  onRestore,
}: {
  open: boolean;
  onClose: () => void;
  /** Project-relative name of the file these revisions belong to. */
  fileLabel: string | null;
  availability: HistoryAvailability;
  rows: RevisionRow[];
  /** Usage against the caps; null while unavailable. */
  summary: string | null;
  /** Cap warning when the list stands at the per-file limit. */
  notice: string | null;
  /** Revision currently being written back, if any. */
  restoringRev: string | null;
  onRestore: (rev: string) => void;
}) {
  return (
    <Dialog open={open} onClose={onClose} maxWidth="sm" fullWidth>
      <DialogTitle sx={{ pb: 0.5 }}>
        History
        {fileLabel ? (
          <Typography variant="caption" color="text.secondary" sx={{ display: 'block' }} noWrap>
            {fileLabel}
          </Typography>
        ) : null}
      </DialogTitle>
      <DialogContent>
        {availability === 'unavailable' ? (
          <Typography variant="body2" color="text.secondary">
            {HISTORY_UNAVAILABLE}
          </Typography>
        ) : rows.length === 0 ? (
          <Typography variant="body2" color="text.secondary">
            {HISTORY_EMPTY}
          </Typography>
        ) : (
          <>
            <Table size="small">
              <TableBody>
                {rows.map((r) => (
                  <TableRow key={r.rev}>
                    <TableCell>
                      {r.when}
                      {r.latest ? (
                        <Typography variant="caption" color="text.secondary" sx={{ ml: 1 }}>
                          latest
                        </Typography>
                      ) : null}
                    </TableCell>
                    <TableCell align="right" sx={{ color: 'text.secondary' }}>
                      {r.size}
                    </TableCell>
                    <TableCell align="right" sx={{ width: 96 }}>
                      <Button
                        size="small"
                        disabled={restoringRev != null}
                        onClick={() => onRestore(r.rev)}
                      >
                        {restoringRev === r.rev ? 'Restoring…' : 'Restore'}
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            {notice ? (
              <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 1 }}>
                {notice}
              </Typography>
            ) : null}
            {summary ? (
              <Typography
                variant="caption"
                color="text.secondary"
                sx={{ display: 'block', mt: notice ? 0.5 : 1 }}
              >
                {summary}
              </Typography>
            ) : null}
          </>
        )}
      </DialogContent>
      <DialogActions>
        <Button variant="contained" onClick={onClose}>
          Close
        </Button>
      </DialogActions>
    </Dialog>
  );
}
