// StatusBar.tsx — fixed 32px bottom bar (phase + timer + main + history).
//
// Constant height; scalar props only (no payload). Compile state lives here.

import Box from '@mui/material/Box';
import LinearProgress from '@mui/material/LinearProgress';
import Typography from '@mui/material/Typography';

export default function StatusBar({
  mainFile,
  mainFileTitle,
  historyCount,
  phase,
  timer,
  message,
}: {
  mainFile?: string | null;
  mainFileTitle?: string | null;
  /** Undoable file-op depth. */
  historyCount?: number;
  phase: string;
  timer: number;
  message: string;
}) {
  const color =
    phase === 'success'
      ? 'success.main'
      : phase === 'failure'
        ? 'error.main'
        : phase === 'compiling'
          ? 'warning.main'
          : 'text.secondary';
  return (
    <Box
      sx={{
        height: 32,
        flexShrink: 0,
        display: 'flex',
        alignItems: 'center',
        gap: 1,
        px: 1,
        borderTop: 1,
        borderColor: 'divider',
        overflow: 'hidden',
      }}
    >
      <Typography variant="caption" sx={{ color, flexShrink: 0 }}>
        ●
      </Typography>
      <Typography
        variant="caption"
        noWrap
        sx={{ flex: '1 1 auto', minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis' }}
      >
        {phase}
        {phase === 'compiling' ? ` ${timer}s` : ''} · {message}
      </Typography>
      {mainFile ? (
        <Typography
          variant="caption"
          noWrap
          title={mainFileTitle ?? mainFile}
          sx={{ flexShrink: 0, maxWidth: '30%', overflow: 'hidden', textOverflow: 'ellipsis' }}
        >
          {mainFile}
        </Typography>
      ) : null}
      {historyCount != null && historyCount > 0 ? (
        <Typography
          variant="caption"
          noWrap
          title={`${historyCount} deleted file${historyCount === 1 ? '' : 's'} restorable (Edit → Undo Delete)`}
          sx={{ flexShrink: 0 }}
        >
          ↩ {historyCount}
        </Typography>
      ) : null}
      {phase === 'compiling' ? (
        <LinearProgress sx={{ width: 120, height: 2, flexShrink: 0 }} />
      ) : null}
    </Box>
  );
}
