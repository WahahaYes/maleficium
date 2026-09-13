// OutlineView.tsx — document outline in the tree column.
//
// Growth cap: entries from `parseOutline` (capped 1000) over the ACTIVE buffer
// only; debounced 500ms upstream via `sourceVersion`. Click → line reveal.

import Box from '@mui/material/Box';
import List from '@mui/material/List';
import ListItemButton from '@mui/material/ListItemButton';
import Typography from '@mui/material/Typography';
import FormatListBulletedIcon from '@mui/icons-material/FormatListBulleted';
import type { OutlineEntry } from '../lib/outline';

export default function OutlineView({ entries, totalShown, onJump }: {
  entries: OutlineEntry[];
  totalShown: string;
  onJump: (line: number) => void;
}) {
  return (
    <Box sx={{ mt: 1 }}>
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, px: 1, py: 0.5 }}>
        <FormatListBulletedIcon fontSize="small" color="action" />
        <Typography variant="caption" color="text.secondary">Outline ({totalShown})</Typography>
      </Box>
      {entries.length === 0 ? (
        <Typography variant="caption" color="text.secondary" sx={{ px: 2 }}>
          No sections in this file.
        </Typography>
      ) : (
        <List dense disablePadding>
          {entries.map((e, i) => (
            <ListItemButton
              key={i}
              onClick={() => onJump(e.line)}
              title={`Go to line ${e.line}`}
              sx={{ pl: 1 + e.level * 1.5, py: 0.25 }}
            >
              <Typography variant="body2" noWrap sx={{ flex: 1 }}>
                {e.title}
              </Typography>
            </ListItemButton>
          ))}
        </List>
      )}
    </Box>
  );
}
