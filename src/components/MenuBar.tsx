// MenuBar.tsx — expandable top menus (File/Edit/Selection/View/Tools/Help).
//
// Growth cap: renders section titles + open menu only; commands are data from
// `lib/commands.ts` (no per-command components). Checked radio items for
// presets/theme; `soon` renders disabled with honest labeling.

import { useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Menu from '@mui/material/Menu';
import MenuItem from '@mui/material/MenuItem';
import ListItemText from '@mui/material/ListItemText';
import Typography from '@mui/material/Typography';
import CheckIcon from '@mui/icons-material/Check';
import type { MenuSection } from '../lib/commands';

export default function MenuBar({ sections, status }: {
  sections: MenuSection[];
  /** Right-side cluster (compile icon + phase) — caller owns the contract. */
  status?: React.ReactNode;
}) {
  const [openId, setOpenId] = useState<string | null>(null);
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);

  const open = (id: string, el: HTMLElement) => {
    setOpenId(id);
    setAnchor(el);
  };
  const close = () => {
    setOpenId(null);
    setAnchor(null);
  };

  return (
    <Box
      sx={{
        display: 'flex', alignItems: 'center', height: 32, flexShrink: 0,
        borderBottom: 1, borderColor: 'divider', px: 0.5,
        backgroundColor: 'background.paper',
      }}
      role="menubar"
      aria-label="Application"
    >
      {sections.map((s) => (
        <Box key={s.id}>
          <Button
            size="small"
            aria-haspopup="menu"
            aria-expanded={openId === s.id}
            onClick={(e) => (openId === s.id ? close() : open(s.id, e.currentTarget))}
            onMouseEnter={(e) => {
              if (openId != null && openId !== s.id) open(s.id, e.currentTarget);
            }}
            sx={{ minWidth: 0, textTransform: 'none' }}
          >
            {s.title}
          </Button>
          <Menu
            open={openId === s.id}
            anchorEl={anchor}
            onClose={close}
            slotProps={{ list: { role: 'menu', 'aria-label': s.title } }}
          >
            {s.commands
              .filter((c) => c.visible !== false)
              .map((c) => (
                <MenuItem
                  key={c.id}
                  role="menuitem"
                  disabled={!c.enabled}
                  onClick={() => { close(); void c.run(); }}
                >
                  {c.checked != null ? (
                    c.checked ? <CheckIcon fontSize="small" /> : <Box sx={{ width: 20 }} />
                  ) : null}
                  <ListItemText>{c.label}</ListItemText>
                  {c.accelerator ? (
                    <Typography variant="caption" color="text.secondary" sx={{ ml: 3, fontFamily: 'monospace' }}>
                      {c.accelerator}
                    </Typography>
                  ) : null}
                </MenuItem>
              ))}
          </Menu>
        </Box>
      ))}
      <Box sx={{ flex: 1 }} />
      {status}
    </Box>
  );
}
