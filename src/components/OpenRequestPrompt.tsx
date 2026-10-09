// OpenRequestPrompt.tsx — an agent asked this window to open a project
// (app_request_open over MCP): Open | Dismiss. Nothing opens without the
// click; the answer goes back to the agent.

import { useCallback, useEffect, useState } from 'react';
import Button from '@mui/material/Button';
import Dialog from '@mui/material/Dialog';
import DialogActions from '@mui/material/DialogActions';
import DialogContent from '@mui/material/DialogContent';
import DialogTitle from '@mui/material/DialogTitle';
import Typography from '@mui/material/Typography';
import { answerOpenRequest, pendingOpenRequest, type OpenRequest } from '../lib/presence';

/** How often the window looks for an ask. */
const POLL_MS = 1000;

export default function OpenRequestPrompt({
  onOpen,
}: {
  /** Open `project` the way File > Open does, then show `file` if given. */
  onOpen: (project: string, file: string | null) => Promise<void>;
}) {
  const [ask, setAsk] = useState<OpenRequest | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    const tick = () =>
      pendingOpenRequest().then(
        (r) => {
          if (live) setAsk((cur) => (cur?.id === r?.id ? cur : r));
        },
        () => {},
      );
    void tick();
    const t = setInterval(() => void tick(), POLL_MS);
    return () => {
      live = false;
      clearInterval(t);
    };
  }, []);

  const answer = useCallback(
    async (open: boolean) => {
      if (!ask || busy) return;
      setBusy(true);
      try {
        if (open) await onOpen(ask.project, ask.file ?? null);
        await answerOpenRequest(ask.id, open ? 'opened' : 'dismissed').catch(() => {});
      } finally {
        setAsk(null);
        setBusy(false);
      }
    },
    [ask, busy, onOpen],
  );

  return (
    <Dialog open={ask != null} onClose={() => void answer(false)} data-testid="open-request-prompt">
      {ask ? (
        <>
          <DialogTitle>Open a project for the agent?</DialogTitle>
          <DialogContent>
            <Typography gutterBottom>
              An AI agent asks this window to open a project folder. It opens only if you choose
              Open, the same as File &gt; Open.
            </Typography>
            <Typography variant="body2" component="div" sx={{ wordBreak: 'break-all' }}>
              {ask.project}
            </Typography>
            {ask.file ? (
              <Typography variant="caption" component="div">
                and shows {ask.file}
              </Typography>
            ) : null}
          </DialogContent>
          <DialogActions>
            <Button
              onClick={() => void answer(false)}
              disabled={busy}
              data-testid="open-request-dismiss"
            >
              Dismiss
            </Button>
            <Button
              variant="contained"
              onClick={() => void answer(true)}
              disabled={busy}
              data-testid="open-request-open"
            >
              Open
            </Button>
          </DialogActions>
        </>
      ) : null}
    </Dialog>
  );
}
