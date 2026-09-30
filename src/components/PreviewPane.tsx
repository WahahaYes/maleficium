import type { ComponentProps } from 'react';
import Box from '@mui/material/Box';
import Preview from './Preview';
import { emit } from '../lib/events';

// The preview column: the PDF view plus the zoom event it reports.
export default function PreviewPane(props: Omit<ComponentProps<typeof Preview>, 'onZoom'>) {
  return (
    <Box
      sx={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        minHeight: 0,
        overflow: 'hidden',
      }}
    >
      <Preview
        {...props}
        onZoom={(mode, percent) =>
          emit({
            scope: 'preview',
            kind: 'info',
            actor: 'user',
            message: `preview zoom ${mode.kind === 'percent' ? '' : mode.kind + ' '}${Math.round(percent)}%`,
            event: { action: 'preview.zoom', mode: mode.kind, percent: Math.round(percent) },
          })
        }
      />
    </Box>
  );
}
