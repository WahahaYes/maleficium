// ExportProgress.tsx — shown while a paper export runs, with Cancel.

import Button from '@mui/material/Button';
import Snackbar from '@mui/material/Snackbar';
import { canCancel, statusText, type ExportRun } from '../lib/exportProgress';

export default function ExportProgress({
  run,
  onCancel,
}: {
  run: ExportRun | null;
  onCancel: () => void;
}) {
  return (
    <Snackbar
      open={run != null}
      message={run ? statusText(run) : ''}
      anchorOrigin={{ vertical: 'bottom', horizontal: 'left' }}
      action={
        <Button color="inherit" size="small" onClick={onCancel} disabled={!canCancel(run)}>
          Cancel
        </Button>
      }
    />
  );
}
