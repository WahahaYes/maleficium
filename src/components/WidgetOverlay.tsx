// WidgetOverlay.tsx — the preview's widget banner: what the shown paper's
// interactive widgets need from the user (running them, then network
// access), or why they stay posters here.

import type { ReactNode } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Typography from '@mui/material/Typography';
import type { WidgetsModel } from '../hooks/useWidgetSession';

export function WidgetBanner({ model }: { model: WidgetsModel }) {
  const { session, approve } = model;
  if (!session || session.entries.length === 0) return null;
  const runnable = session.entries.filter((e) => e.poster !== 'no-runtime');
  const wantNet = runnable.filter((e) => e.declaresNetwork);
  let text: string | null = null;
  let action: ReactNode = null;
  if (session.unavailable.length > 0) {
    text = 'Interactive widgets show as posters here: this system has no widget host for them.';
  } else if (runnable.length === 0) {
    return null;
  } else if (!session.approval.run) {
    const n = runnable.length;
    text = `This paper has ${n} interactive widget${n === 1 ? '' : 's'}. They stay posters until you let them run.`;
    action = (
      <Button size="small" onClick={() => approve('run', true)}>
        Run widgets
      </Button>
    );
  } else if (wantNet.length > 0 && !session.approval.network) {
    const n = wantNet.length;
    text = `${n} widget${n === 1 ? ' asks' : 's ask'} for network access. They run offline until you allow it.`;
    action = (
      <Button size="small" onClick={() => approve('network', true)}>
        Allow network
      </Button>
    );
  }
  if (!text) return null;
  return (
    <Box
      data-widget-banner=""
      sx={{
        display: 'flex',
        alignItems: 'center',
        gap: 1,
        px: 1.5,
        py: 0.5,
        borderBottom: 1,
        borderColor: 'divider',
        bgcolor: 'background.paper',
      }}
    >
      <Typography variant="body2" sx={{ flex: 1 }}>
        {text}
      </Typography>
      {action}
    </Box>
  );
}
