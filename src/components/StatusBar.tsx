// StatusBar.tsx — fixed 32px bottom bar (phase + timer + offline badge + main
// + restorables).
//
// Constant height; scalar props only (no payload). Compile state lives here.
// The revision count is the entry point to the History surface.

import Box from '@mui/material/Box';
import ButtonBase from '@mui/material/ButtonBase';
import LinearProgress from '@mui/material/LinearProgress';
import Typography from '@mui/material/Typography';
import type { OfflineBadge } from '../lib/compile';

export default function StatusBar({
  mainFile,
  mainFileTitle,
  historyCount,
  revisionCount,
  revisionTitle,
  onOpenHistory,
  phase,
  timer,
  message,
  offline,
}: {
  mainFile?: string | null;
  mainFileTitle?: string | null;
  /** Undoable file-op depth. */
  historyCount?: number;
  /** Revisions kept for the active file. */
  revisionCount?: number;
  revisionTitle?: string;
  onOpenHistory?: () => void;
  phase: string;
  timer: number;
  message: string;
  /** The project's offline readiness; absent outside a project. */
  offline?: OfflineBadge | null;
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
      {offline ? (
        <Typography
          variant="caption"
          noWrap
          title={offline.title}
          data-testid="offline-badge"
          sx={{
            flexShrink: 0,
            maxWidth: '30%',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            px: 0.75,
            border: 1,
            borderRadius: 1,
            borderColor: 'divider',
            color:
              offline.tone === 'success'
                ? 'success.main'
                : offline.tone === 'warning'
                  ? 'warning.main'
                  : offline.tone === 'error'
                    ? 'error.main'
                    : 'text.secondary',
          }}
        >
          {offline.label}
        </Typography>
      ) : null}
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
      {onOpenHistory && revisionCount != null && revisionCount > 0 ? (
        <ButtonBase
          onClick={onOpenHistory}
          title={revisionTitle ?? `${revisionCount} revisions — open History`}
          sx={{
            flexShrink: 0,
            px: 0.5,
            borderRadius: 0.5,
            typography: 'caption',
            color: 'text.secondary',
            '&:hover': { color: 'text.primary', bgcolor: 'action.hover' },
          }}
        >
          ⟲ {revisionCount}
        </ButtonBase>
      ) : null}
      {phase === 'compiling' ? (
        <LinearProgress sx={{ width: 120, height: 2, flexShrink: 0 }} />
      ) : null}
    </Box>
  );
}
