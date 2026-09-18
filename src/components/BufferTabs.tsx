// BufferTabs.tsx — visible open-file tabs (buffers Map made tangible).
//
// Growth cap: tab strip renders one chip per OPEN buffer (bounded by user
// behavior, not artifact size); labels are basenames; close evicts from map.

import Box from '@mui/material/Box';
import Chip from '@mui/material/Chip';
import IconButton from '@mui/material/IconButton';
import Menu from '@mui/material/Menu';
import MenuItem from '@mui/material/MenuItem';
import CloseIcon from '@mui/icons-material/Close';
import { useState } from 'react';

export default function BufferTabs({
  buffers,
  active,
  onSelect,
  onClose,
  onCloseOthers,
  onCloseAll,
}: {
  buffers: Map<string, { dirty: boolean }>;
  active: string;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
  onCloseOthers?: (keep: string) => void;
  onCloseAll?: () => void;
}) {
  const [menuAnchor, setMenuAnchor] = useState<HTMLElement | null>(null);
  if (buffers.size === 0) return null;
  const base = (p: string) => p.slice(p.lastIndexOf('/') + 1) || p;
  // Stable order: insertion order of the Map (open order).
  return (
    <Box
      role="tablist"
      aria-label="Open files"
      sx={{ display: 'flex', gap: 0.5, overflowX: 'auto', py: 0.5, flexShrink: 0 }}
      onKeyDown={(e) => {
        if (e.key !== 'Tab' || !e.ctrlKey) return;
        e.preventDefault();
        const keys = [...buffers.keys()];
        const i = keys.indexOf(active);
        const n = e.shiftKey ? (i - 1 + keys.length) % keys.length : (i + 1) % keys.length;
        onSelect(keys[n]);
      }}
    >
      {[...buffers.keys()].map((p) => {
        const dirty = buffers.get(p)?.dirty;
        return (
          <Chip
            key={p}
            role="tab"
            aria-selected={p === active}
            label={`${base(p)}${dirty ? ' ●' : ''}`}
            title={p}
            size="small"
            color={p === active ? 'primary' : 'default'}
            variant={p === active ? 'filled' : 'outlined'}
            onClick={() => onSelect(p)}
            onDelete={() => onClose(p)}
            sx={{ maxWidth: 180 }}
          />
        );
      })}
      {buffers.size > 1 && (onCloseOthers || onCloseAll) ? (
        <>
          <IconButton
            size="small"
            aria-label="Close tabs menu"
            title="Close tabs…"
            onClick={(e) => setMenuAnchor(e.currentTarget)}
            sx={{ flexShrink: 0 }}
          >
            <CloseIcon fontSize="small" />
          </IconButton>
          <Menu open={menuAnchor != null} anchorEl={menuAnchor} onClose={() => setMenuAnchor(null)}>
            {onCloseOthers ? (
              <MenuItem
                onClick={() => {
                  setMenuAnchor(null);
                  onCloseOthers(active);
                }}
              >
                Close others
              </MenuItem>
            ) : null}
            {onCloseAll ? (
              <MenuItem
                onClick={() => {
                  setMenuAnchor(null);
                  onCloseAll();
                }}
              >
                Close all
              </MenuItem>
            ) : null}
          </Menu>
        </>
      ) : null}
    </Box>
  );
}
