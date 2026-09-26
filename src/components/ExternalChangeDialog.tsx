// ExternalChangeDialog.tsx — an open file changed on disk under unsaved edits.
//
// One conflict at a time; Escape keeps the user's edits.

import Button from '@mui/material/Button';
import Dialog from '@mui/material/Dialog';
import DialogActions from '@mui/material/DialogActions';
import DialogContent from '@mui/material/DialogContent';
import DialogContentText from '@mui/material/DialogContentText';
import DialogTitle from '@mui/material/DialogTitle';

export default function ExternalChangeDialog({
  path,
  more,
  onReload,
  onKeep,
}: {
  /** Display path of the conflicted file. */
  path: string;
  /** Further conflicts waiting behind this one. */
  more: number;
  onReload: () => void;
  onKeep: () => void;
}) {
  return (
    <Dialog open onClose={onKeep} maxWidth="xs" fullWidth data-testid="external-change-dialog">
      <DialogTitle>File changed on disk</DialogTitle>
      <DialogContent>
        <DialogContentText>
          <strong>{path}</strong> was changed outside Maleficium while it has unsaved edits.
        </DialogContentText>
        <DialogContentText sx={{ mt: 1 }}>
          Reload to take the disk version and discard your edits, or keep your edits and replace the
          disk version when you next save.
          {more > 0 ? ` ${more} more file${more === 1 ? '' : 's'} waiting.` : ''}
        </DialogContentText>
      </DialogContent>
      <DialogActions>
        <Button onClick={onKeep}>Keep my edits</Button>
        <Button variant="contained" onClick={onReload}>
          Reload from disk
        </Button>
      </DialogActions>
    </Dialog>
  );
}
