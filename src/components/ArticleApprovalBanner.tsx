// ArticleApprovalBanner.tsx — the Article tab's pending-approval banner.
//
// Shown while the widget store holds widgets or runtimes the user has not
// approved: the article renders those as posters. It names what is held
// and opens View > Widgets; it has no dismissal, and an approval clears it.

import Alert from '@mui/material/Alert';
import Button from '@mui/material/Button';
import { WIDGETS_PANEL_PATH } from '../lib/widgets.view';

export default function ArticleApprovalBanner({
  text,
  onReview,
}: {
  /** One sentence naming the held widgets and runtimes. */
  text: string;
  /** Opens View > Widgets to review the held items. */
  onReview: () => void;
}) {
  return (
    <Alert
      severity="warning"
      data-testid="article-approval-banner"
      action={
        <Button size="small" onClick={onReview} data-testid="article-approval-review">
          Review in {WIDGETS_PANEL_PATH}
        </Button>
      }
      sx={{ alignItems: 'center' }}
    >
      {text}
    </Alert>
  );
}
