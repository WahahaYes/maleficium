// ActivityBar.tsx — 48px left rail (Pitch 1, activity-bar navigation).
//
// Growth cap: constant width, icon-only, no unbounded content.

import Stack from '@mui/material/Stack';
import IconButton from '@mui/material/IconButton';
import Tooltip from '@mui/material/Tooltip';
import Typography from '@mui/material/Typography';

export type ActivityMode = 'file' | 'project' | 'preview';

export default function ActivityBar({ mode, onMode }: { mode: ActivityMode; onMode: (m: ActivityMode) => void }) {
  const items: { m: ActivityMode; icon: string; tip: string }[] = [
    { m: 'file', icon: '📁', tip: 'Explorer (files + editor)' },
    { m: 'project', icon: '📦', tip: 'Project (files + preview)' },
    { m: 'preview', icon: '📄', tip: 'Preview (editor + preview)' },
  ];
  return (
    <Stack spacing={0} sx={{ width: 48, flexShrink: 0, alignItems: 'center', py: 1, borderRight: 1, borderColor: 'divider' }}>
      {items.map((it) => (
        <Tooltip key={it.m} title={it.tip} placement="right">
          <IconButton
            size="small"
            aria-label={it.tip}
            color={mode === it.m ? 'primary' : 'default'}
            onClick={() => onMode(it.m)}
            sx={{ borderLeft: mode === it.m ? 2 : 0, borderColor: 'primary.main', borderRadius: 0 }}
          >
            <Typography variant="body2" aria-hidden>{it.icon}</Typography>
          </IconButton>
        </Tooltip>
      ))}
    </Stack>
  );
}
