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
  runtimePromptBody,
  runtimePromptTitle,
  shortDigest,
  WIDGETS_PANEL_PATH,
  type ApprovalPrompt,
  type RuntimePrompt,
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

// RuntimeApprovalPrompt.tsx (same file) — asked once per pending runtime
// before an export runs: Allow | Deny | Not now. Closing the dialog is Not
// now: no write, this export stays poster-only, next export asks again.

export function RuntimeApprovalPrompt({
  prompt,
  failure,
  onAllow,
  onDeny,
  onNotNow,
  onReview,
  onDismissFailure,
}: {
  prompt: RuntimePrompt | null;
  /** Why the last decision was refused, if it was. */
  failure: string | null;
  onAllow: () => void;
  onDeny: () => void;
  onNotNow: () => void;
  /** Open View > Widgets to review the package first, when offered. */
  onReview?: () => void;
  onDismissFailure: () => void;
}) {
  return (
    <>
      <Dialog open={prompt != null} onClose={onNotNow} data-testid="runtime-approval-prompt">
        {prompt ? (
          <>
            <DialogTitle>{runtimePromptTitle(prompt)}</DialogTitle>
            <DialogContent>
              <Typography gutterBottom>{runtimePromptBody(prompt)}</Typography>
              <Typography variant="caption" component="div">
                {causeDetail(prompt.cause)}
              </Typography>
              <Typography variant="body2" component="div" sx={{ mt: 1 }}>
                Version {prompt.version || 'unknown'} · digest{' '}
                <span title={prompt.digest}>{shortDigest(prompt.digest)}</span> · licence{' '}
                {prompt.license || 'unknown'} · WebGL {prompt.webgl ? 'yes' : 'no'}
              </Typography>
              <Typography variant="caption" component="div">
                Used by: {prompt.widgets.join(', ')}
              </Typography>
              {prompt.vendored.length ? (
                <Typography variant="caption" component="div">
                  Vendored:{' '}
                  {prompt.vendored.map((v) => `${v.name} ${v.version} (${v.license})`).join('; ')}
                </Typography>
              ) : null}
              {prompt.warnings.length ? (
                <Typography variant="caption" component="div">
                  Validator warnings: {prompt.warnings.join('; ')}
                </Typography>
              ) : null}
              <Typography variant="caption" component="div" sx={{ mt: 1 }}>
                Not now exports poster-only and asks again next export.
                {onReview
                  ? ' Review the package source first in ' + WIDGETS_PANEL_PATH + '.'
                  : null}
              </Typography>
            </DialogContent>
            <DialogActions>
              {onReview ? (
                <Button onClick={onReview} data-testid="runtime-prompt-review">
                  Review
                </Button>
              ) : null}
              <Button onClick={onNotNow} data-testid="runtime-prompt-not-now">
                Not now
              </Button>
              <Button color="error" onClick={onDeny} data-testid="runtime-prompt-deny">
                Deny
              </Button>
              <Button variant="contained" onClick={onAllow} data-testid="runtime-prompt-allow">
                Allow
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
