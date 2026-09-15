// OutlineView.tsx — document outline in the tree column.
//
// Growth cap: entries from `parseOutline` (DATA cap 1000) sliced to a VIEW
// cap of 100 rows AFTER filtering (the filter segments, the cap bounds the
// list — filtering first keeps each segment useful, capping second keeps
// rows O(visible)). The `100+` count line keeps the cut honest, and the
// remainder stays reachable via Selection > Go to Section… (25-row window
// over the same list). Click → line reveal.
//
// D-12 design: sections own the indent hierarchy; labels / floats / inputs
// ride along as marker rows with a kind glyph + muted meta. Markers never
// change a section's level (the parse guarantees it), so the indent column
// stays a pure section tree and the filter segments stay truthful.

import { useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import ButtonGroup from '@mui/material/ButtonGroup';
import List from '@mui/material/List';
import ListItemButton from '@mui/material/ListItemButton';
import ListItemIcon from '@mui/material/ListItemIcon';
import Typography from '@mui/material/Typography';
import FormatListBulletedIcon from '@mui/icons-material/FormatListBulleted';
import TagIcon from '@mui/icons-material/Tag';
import ImageIcon from '@mui/icons-material/Image';
import TableChartIcon from '@mui/icons-material/TableChart';
import InputIcon from '@mui/icons-material/Input';
import type { OutlineEntry, OutlineFilter, OutlineKind } from '../lib/outline';
import { filterOutline } from '../lib/outline';

const VIEW_CAP = 100;

function KindGlyph({ kind }: { kind: OutlineKind }) {
  // Sections need no glyph (indent IS their signal); markers get one small
  // muted icon so a scan reads kind before text.
  switch (kind) {
    case 'label': return <TagIcon fontSize="small" color="action" sx={{ fontSize: 14 }} />;
    case 'figure': return <ImageIcon fontSize="small" color="action" sx={{ fontSize: 14 }} />;
    case 'table': return <TableChartIcon fontSize="small" color="action" sx={{ fontSize: 14 }} />;
    case 'input': return <InputIcon fontSize="small" color="action" sx={{ fontSize: 14 }} />;
    default: return null;
  }
}

function RowMeta({ entry }: { entry: OutlineEntry }) {
  // One muted word per marker: label key / file path / input path. Sections
  // show nothing (their title is the whole row — minimal, sleek).
  if (entry.kind === 'section') return null;
  const meta = entry.kind === 'label' ? entry.detail
    : entry.kind === 'input' ? entry.detail
    : entry.detail;
  if (!meta || meta === entry.title) return null;
  return (
    <Typography variant="caption" color="text.secondary" noWrap sx={{ ml: 1, flexShrink: 0, maxWidth: '40%' }}>
      {meta}
    </Typography>
  );
}

const SEGMENTS: { id: OutlineFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'sections', label: 'Sections' },
  { id: 'labels', label: 'Labels' },
  { id: 'figures', label: 'Figures' },
  { id: 'inputs', label: 'Inputs' },
];

export default function OutlineView({ entries, onJump }: {
  entries: OutlineEntry[];
  onJump: (line: number) => void;
}) {
  const [filter, setFilter] = useState<OutlineFilter>('all');
  const filtered = filterOutline(entries, filter);
  const shown = filtered.slice(0, VIEW_CAP);
  const totalLabel = filtered.length > VIEW_CAP ? `${VIEW_CAP}+` : String(filtered.length);
  return (
    <Box sx={{ mt: 1 }}>
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, px: 1, py: 0.5 }}>
        <FormatListBulletedIcon fontSize="small" color="action" />
        <Typography variant="caption" color="text.secondary">Outline ({totalLabel})</Typography>
      </Box>
      <ButtonGroup size="small" sx={{ px: 1, pb: 0.5 }} aria-label="Outline filter">
        {SEGMENTS.map((s) => (
          <Button
            key={s.id}
            variant={filter === s.id ? 'contained' : 'outlined'}
            aria-pressed={filter === s.id}
            onClick={() => setFilter(s.id)}
            sx={{ minWidth: 0, px: 1, fontSize: 11 }}
          >
            {s.label}
          </Button>
        ))}
      </ButtonGroup>
      {shown.length === 0 ? (
        <Typography variant="caption" color="text.secondary" sx={{ px: 2 }}>
          {entries.length === 0 ? 'No sections in this file.' : `No ${filter} in this file.`}
        </Typography>
      ) : (
        <List dense disablePadding>
          {shown.map((e, i) => (
            <ListItemButton
              key={`${e.line}-${i}`}
              onClick={() => onJump(e.line)}
              title={`Go to line ${e.line}`}
              sx={{ pl: 1 + e.level * 1.5, py: 0.25 }}
            >
              {e.kind !== 'section' ? (
                <ListItemIcon sx={{ minWidth: 22 }}>
                  <KindGlyph kind={e.kind} />
                </ListItemIcon>
              ) : null}
              <Typography variant="body2" noWrap sx={{ flex: 1, minWidth: 0 }}>
                {e.title}
              </Typography>
              <RowMeta entry={e} />
            </ListItemButton>
          ))}
        </List>
      )}
    </Box>
  );
}
