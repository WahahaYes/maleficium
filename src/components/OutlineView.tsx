// OutlineView.tsx — document outline, one visual section in the side column.
//
// No panel chrome of its own: a caption line, a search field, and rows.
// Entries are searched, filtered by kind segment, then sliced to a 100-row
// view (search narrows first so the cap never eats the match; the `100+`
// count line keeps the cut honest). Click → line reveal.
//
// Sections own the indent hierarchy; labels / floats / inputs ride along
// as marker rows with a kind glyph + muted meta. Markers never change a
// section's level, so the indent column stays a pure section tree.

import { useState } from 'react';
import Box from '@mui/material/Box';
import List from '@mui/material/List';
import ListItemButton from '@mui/material/ListItemButton';
import ListItemIcon from '@mui/material/ListItemIcon';
import TextField from '@mui/material/TextField';
import ToggleButton from '@mui/material/ToggleButton';
import ToggleButtonGroup from '@mui/material/ToggleButtonGroup';
import Typography from '@mui/material/Typography';
import TagIcon from '@mui/icons-material/Tag';
import ImageIcon from '@mui/icons-material/Image';
import TableChartIcon from '@mui/icons-material/TableChart';
import InputIcon from '@mui/icons-material/Input';
import type { OutlineEntry, OutlineFilter, OutlineKind } from '../lib/outline';
import { filterOutline, searchOutline } from '../lib/outline';

const VIEW_CAP = 100;

function KindGlyph({ kind }: { kind: OutlineKind }) {
  // Sections need no glyph (indent is their signal); markers get one small
  // muted icon so a scan reads kind before text.
  switch (kind) {
    case 'label':
      return <TagIcon fontSize="small" color="action" sx={{ fontSize: 14 }} />;
    case 'figure':
      return <ImageIcon fontSize="small" color="action" sx={{ fontSize: 14 }} />;
    case 'table':
      return <TableChartIcon fontSize="small" color="action" sx={{ fontSize: 14 }} />;
    case 'input':
      return <InputIcon fontSize="small" color="action" sx={{ fontSize: 14 }} />;
    default:
      return null;
  }
}

function RowMeta({ entry }: { entry: OutlineEntry }) {
  // One muted word per marker: label key / file path / input path. Sections
  // show nothing (their title is the whole row).
  if (entry.kind === 'section') return null;
  const meta =
    entry.kind === 'label' ? entry.detail : entry.kind === 'input' ? entry.detail : entry.detail;
  if (!meta || meta === entry.title) return null;
  return (
    <Typography
      variant="caption"
      color="text.secondary"
      noWrap
      sx={{ ml: 1, flexShrink: 0, maxWidth: '40%' }}
    >
      {meta}
    </Typography>
  );
}

const SEGMENTS: { id: OutlineFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'sections', label: '§' },
  { id: 'labels', label: '#' },
  { id: 'figures', label: 'Fig' },
  { id: 'inputs', label: 'In' },
];

export default function OutlineView({
  entries,
  onJump,
}: {
  entries: OutlineEntry[];
  onJump: (line: number) => void;
}) {
  const [filter, setFilter] = useState<OutlineFilter>('all');
  const [query, setQuery] = useState('');
  const searched = searchOutline(entries, query);
  const filtered = filterOutline(searched, filter);
  const shown = filtered.slice(0, VIEW_CAP);
  const totalLabel = filtered.length > VIEW_CAP ? `${VIEW_CAP}+` : String(filtered.length);
  return (
    <Box sx={{ mt: 1 }}>
      <Typography
        variant="caption"
        color="text.secondary"
        sx={{ display: 'block', px: 1, pb: 0.5 }}
      >
        Outline ({totalLabel})
      </Typography>
      <Box sx={{ display: 'flex', gap: 0.5, px: 1, pb: 0.5 }}>
        <TextField
          size="small"
          fullWidth
          placeholder="Filter symbols"
          aria-label="Filter symbols"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          sx={{ '& .MuiInputBase-input': { py: 0.5, fontSize: 12 } }}
        />
        <ToggleButtonGroup
          size="small"
          exclusive
          value={filter}
          onChange={(_, v: OutlineFilter | null) => {
            if (v) setFilter(v);
          }}
          aria-label="Symbol kind"
          sx={{ flexShrink: 0, '& .MuiToggleButton-root': { px: 0.75, py: 0.5, fontSize: 11 } }}
        >
          {SEGMENTS.map((s) => (
            <ToggleButton key={s.id} value={s.id} aria-label={s.id} title={s.id}>
              {s.label}
            </ToggleButton>
          ))}
        </ToggleButtonGroup>
      </Box>
      {shown.length === 0 ? (
        <Typography variant="caption" color="text.secondary" sx={{ px: 2 }}>
          {entries.length === 0 ? 'No sections in this file.' : 'No matches.'}
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
