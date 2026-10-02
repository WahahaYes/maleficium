// WidgetApprovalPrompt.tsx — asked when a compile finds an html widget the
// user has not approved: Approve | Skip. Approving here covers
// exactly the version named; View > Widgets has the source diff.

import Button from '@mui/material/Button';
import Dialog from '@mui/material/Dialog';
import DialogActions from '@mui/material/DialogActions';
import DialogContent from '@mui/material/DialogContent';
import DialogTitle from '@mui/material/DialogTitle';
import Snackbar from '@mui/material/Snackbar';
import Typography from '@mui/material/Typography';
import {
  causeDetail,
  shortDigest,
  WIDGETS_PANEL_PATH,
  type ApprovalPrompt,
} from '../lib/widgets.view';

export default function WidgetApprovalPrompt({
  prompt,
  failure,
  onApprove,
  onSkip,
  onDismissFailure,
}: {
  prompt: ApprovalPrompt | null;
  /** Why the last approval was refused, if it was. */
  failure: string | null;
  onApprove: () => void;
  onSkip: () => void;
  onDismissFailure: () => void;
}) {
  return (
    <>
      <Dialog open={prompt != null} onClose={onSkip} data-testid="widget-approval-prompt">
        {prompt ? (
          <>
            <DialogTitle>Approve widget &lsquo;{prompt.widget}&rsquo;?</DialogTitle>
            <DialogContent>
              <Typography gutterBottom>
                This widget&rsquo;s own HTML and scripts run when the app renders it.{' '}
                {causeDetail(prompt.cause)}
              </Typography>
              <Typography variant="caption" component="div">
                {prompt.path} · digest {shortDigest(prompt.digest)}
              </Typography>
              {prompt.origins.length ? (
                <Typography variant="caption" component="div">
                  Declared origins: {prompt.origins.join('; ')}
                </Typography>
              ) : null}
              <Typography variant="caption" component="div" sx={{ mt: 1 }}>
                Skip to review its source first in {WIDGETS_PANEL_PATH}.
              </Typography>
            </DialogContent>
            <DialogActions>
              <Button onClick={onSkip} data-testid="widget-prompt-skip">
                Skip
              </Button>
              <Button variant="contained" onClick={onApprove} data-testid="widget-prompt-approve">
                Approve
              </Button>
            </DialogActions>
          </>
        ) : null}
      </Dialog>
      <Snackbar
        open={failure != null}
        message={failure ?? ''}
        onClose={onDismissFailure}
        autoHideDuration={8000}
      />
    </>
  );
}
