// ExportReportDialog.tsx — what an export wrote and every warning it raised.
//
// Warnings never block the export; this is where they are read in full.
// Rendered by kind with its message, so a new kind needs no change here.

import Button from '@mui/material/Button';
import Dialog from '@mui/material/Dialog';
import DialogActions from '@mui/material/DialogActions';
import DialogContent from '@mui/material/DialogContent';
import DialogTitle from '@mui/material/DialogTitle';
import Typography from '@mui/material/Typography';
import type { BundleReport } from '../lib/bundleReport';

export default function ExportReportDialog({
  report,
  onClose,
}: {
  report: BundleReport | null;
  onClose: () => void;
}) {
  const n = report?.count ?? 0;
  return (
    <Dialog open={report != null} onClose={onClose} maxWidth="sm" fullWidth>
      <DialogTitle>
        {n === 0 ? 'Export finished' : `Export finished with ${n} warning${n === 1 ? '' : 's'}`}
      </DialogTitle>
      <DialogContent dividers>
        {report && (
          <>
            <Typography variant="body2" sx={{ mb: 1, wordBreak: 'break-all' }}>
              Wrote a {report.profile} bundle to {report.path} ({report.bytes} bytes).
            </Typography>
            {n > 0 && (
              <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
                The paper was still exported. These are the things to check.
              </Typography>
            )}
            {report.groups.map((g) => (
              <section key={g.kind} aria-label={g.label} data-kind={g.kind}>
                <Typography variant="subtitle2" sx={{ mt: 1.5 }}>
                  {g.label} ({g.messages.length})
                </Typography>
                <ul style={{ margin: '4px 0 0', paddingLeft: 20 }}>
                  {g.messages.map((m, i) => (
                    <li key={i}>
                      <Typography variant="body2" sx={{ overflowWrap: 'anywhere' }}>
                        {m}
                      </Typography>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </>
        )}
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} autoFocus>
          Close
        </Button>
      </DialogActions>
    </Dialog>
  );
}
