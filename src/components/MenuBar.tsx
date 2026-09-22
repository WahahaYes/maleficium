// MenuBar.tsx — expandable top menus (File/Edit/Selection/View/Tools/Help).
//
// Renders section titles + open menu only; commands are data (no
// per-command components). `checked` = radio/toggle state; `children` =
// nested submenu (one level).

import { useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Menu from '@mui/material/Menu';
import MenuItem from '@mui/material/MenuItem';
import ListItemText from '@mui/material/ListItemText';
import Typography from '@mui/material/Typography';
import CheckIcon from '@mui/icons-material/Check';
import ChevronRightIcon from '@mui/icons-material/ChevronRight';
import type { MenuCommand, MenuSection } from '../lib/commands';
import { typeScale } from '../lib/theme';

function Row({ c, close }: { c: MenuCommand; close: () => void }) {
  const [subAnchor, setSubAnchor] = useState<HTMLElement | null>(null);
  const subOpen = subAnchor != null;

  if (c.children && c.children.length > 0) {
    return (
      <>
        <MenuItem
          role="menuitem"
          aria-haspopup="menu"
          aria-expanded={subOpen}
          disabled={!c.enabled}
          onClick={(e) => setSubAnchor(e.currentTarget)}
          onMouseEnter={(e) => {
            if (c.enabled) setSubAnchor(e.currentTarget);
          }}
        >
          {c.checked != null ? (
            c.checked ? (
              <CheckIcon fontSize="small" />
            ) : (
              <Box sx={{ width: 20 }} />
            )
          ) : null}
          <ListItemText>{c.label}</ListItemText>
          <ChevronRightIcon fontSize="small" color="action" />
        </MenuItem>
        <Menu
          open={subOpen && c.enabled}
          anchorEl={subAnchor}
          anchorOrigin={{ vertical: 'top', horizontal: 'right' }}
          onClose={() => setSubAnchor(null)}
          slotProps={{ list: { role: 'menu', 'aria-label': c.label } }}
        >
          {c.children.map((k, i) => (
            <MenuItem
              key={`${k.id}-${i}`}
              role="menuitemradio"
              aria-checked={k.checked ?? undefined}
              disabled={!k.enabled}
              onClick={() => {
                setSubAnchor(null);
                close();
                void k.run?.();
              }}
            >
              {k.checked != null ? (
                k.checked ? (
                  <CheckIcon fontSize="small" />
                ) : (
                  <Box sx={{ width: 20 }} />
                )
              ) : null}
              <ListItemText>{k.label}</ListItemText>
              {k.accelerator ? (
                <Typography
                  variant="caption"
                  color="text.secondary"
                  sx={{ ml: 3, fontFamily: typeScale.uiMonoFamily }}
                >
                  {k.accelerator}
                </Typography>
              ) : null}
            </MenuItem>
          ))}
        </Menu>
      </>
    );
  }

  return (
    <MenuItem
      role="menuitem"
      disabled={!c.enabled}
      onClick={() => {
        close();
        void c.run?.();
      }}
    >
      {c.checked != null ? (
        c.checked ? (
          <CheckIcon fontSize="small" />
        ) : (
          <Box sx={{ width: 20 }} />
        )
      ) : null}
      <ListItemText>{c.label}</ListItemText>
      {c.accelerator ? (
        <Typography
          variant="caption"
          color="text.secondary"
          sx={{ ml: 3, fontFamily: typeScale.uiMonoFamily }}
        >
          {c.accelerator}
        </Typography>
      ) : null}
    </MenuItem>
  );
}

export default function MenuBar({
  sections,
  status,
}: {
  sections: MenuSection[];
  /** Right-side cluster (compile icon + phase). */
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
        display: 'flex',
        alignItems: 'center',
        height: 32,
        flexShrink: 0,
        borderBottom: 1,
        borderColor: 'divider',
        px: 0.5,
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
                <Row key={c.id} c={c} close={close} />
              ))}
          </Menu>
        </Box>
      ))}
      <Box sx={{ flex: 1 }} />
      {status}
    </Box>
  );
}
