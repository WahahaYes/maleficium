// StatusBar.tsx — fixed 32px bottom bar (phase + timer + main + git).
//
// Growth cap: constant height; scalar props only (mainFile string, phase enum,
// timer number); no payload. Compile state placed here (01 contract preserved).

import Box from '@mui/material/Box';
import LinearProgress from '@mui/material/LinearProgress';
import Typography from '@mui/material/Typography';

export default function StatusBar({ mainFile, gitBranch, ahead, phase, timer, message }: {
  mainFile?: string | null;
  gitBranch?: string | null;
  ahead?: number | null;
  phase: string;
  timer: number;
  message: string;
}) {
  const color = phase === 'success' ? 'success.main' : phase === 'failure' ? 'error.main' : phase === 'compiling' ? 'warning.main' : 'text.secondary';
  return (
    <Box sx={{ height: 32, flexShrink: 0, display: 'flex', alignItems: 'center', gap: 1, px: 1, borderTop: 1, borderColor: 'divider' }}>
      <Typography variant="caption" sx={{ color }}>●</Typography>
      <Typography variant="caption" noWrap>
        {phase}{phase === 'compiling' ? ` ${timer}s` : ''} · {message}
      </Typography>
      {mainFile ? <Typography variant="caption" noWrap sx={{ ml: 'auto' }}>{mainFile}</Typography> : null}
      {gitBranch ? <Typography variant="caption" noWrap>{gitBranch}{ahead ? ` ↑${ahead}` : ''}</Typography> : null}
      {phase === 'compiling' ? <LinearProgress sx={{ width: 120, height: 2 }} /> : null}
    </Box>
  );
}
