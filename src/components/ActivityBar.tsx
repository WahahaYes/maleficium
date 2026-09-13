// ActivityBar.tsx — 48px left rail (Pitch 1, activity-bar navigation).
//
// Growth cap: constant width, icon-only, no unbounded content.

import Stack from '@mui/material/Stack';
import IconButton from '@mui/material/IconButton';
import Tooltip from '@mui/material/Tooltip';
import FolderIcon from '@mui/icons-material/Folder';
import ViewSidebarIcon from '@mui/icons-material/ViewSidebar';
import PictureAsPdfIcon from '@mui/icons-material/PictureAsPdf';

export type ActivityMode = 'file' | 'project' | 'preview';

export default function ActivityBar({ mode, onMode }: { mode: ActivityMode; onMode: (m: ActivityMode) => void }) {
  const items = [
    { m: 'file' as ActivityMode, tip: 'Explorer (files + editor)', Icon: FolderIcon },
    { m: 'project' as ActivityMode, tip: 'Project (files + preview)', Icon: ViewSidebarIcon },
    { m: 'preview' as ActivityMode, tip: 'Preview (editor + preview)', Icon: PictureAsPdfIcon },
  ];
  return (
    <Stack spacing={0} sx={{ width: 48, flexShrink: 0, alignItems: 'center', py: 1, borderRight: 1, borderColor: 'divider' }}>
      {items.map(({ m, tip, Icon }) => (
        <Tooltip key={m} title={tip} placement="right">
          <IconButton
            size="small"
            aria-label={tip}
            color={mode === m ? 'primary' : 'default'}
            onClick={() => onMode(m)}
            sx={{ borderLeft: mode === m ? 2 : 0, borderColor: 'primary.main', borderRadius: 0 }}
          >
            <Icon fontSize="small" />
          </IconButton>
        </Tooltip>
      ))}
    </Stack>
  );
}
