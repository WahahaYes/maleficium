// LogStream.tsx — the single unified log event stream.
//
// Every surface emits here; `compile.problem` entries inside a session root
// render as click-to-jump rows. The bus keeps the newest 500 events; the
// view refreshes four times a second and renders the newest 100.

import { useEffect, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import List from '@mui/material/List';
import ListItem from '@mui/material/ListItem';
import ListItemButton from '@mui/material/ListItemButton';
import Typography from '@mui/material/Typography';
import { tailEvents, eventOf } from '../lib/events';
import { transport } from '../lib/event-transport';
import type { BusEvent } from '../lib/generated/events';
import { typeScale } from '../lib/theme';

/** A problem's jump target: root-relative, inside a known session root. */
export interface ProblemRef {
  rootId: string;
  path: string;
  line: number;
}

function problemRefOf(e: BusEvent): ProblemRef | null {
  const p = eventOf(e, 'compile.problem');
  if (!p || p.external || p.rootId === null || p.path === undefined) return null;
  return { rootId: p.rootId, path: p.path, line: p.line };
}

function kindColor(kind: BusEvent['kind']): string {
  switch (kind) {
    case 'error':
      return 'error.main';
    case 'warn':
      return 'warning.main';
    case 'success':
      return 'success.main';
    default:
      return 'text.primary';
  }
}

function RowText({ e }: { e: BusEvent }) {
  return (
    <Typography
      variant="body2"
      sx={{
        fontFamily: typeScale.uiMonoFamily,
        fontSize: typeScale.dense,
        color: kindColor(e.kind),
        overflowWrap: 'anywhere',
      }}
    >
      {new Date(e.at).toLocaleTimeString()} {e.message}
    </Typography>
  );
}

export default function LogStream({
  height,
  onHeight,
  collapsed,
  onToggleCollapse,
  onJump,
}: {
  height: number;
  onHeight: (h: number) => void;
  collapsed: boolean;
  onToggleCollapse: () => void;
  onJump: (target: ProblemRef) => void;
}) {
  const [evts, setEvts] = useState<BusEvent[]>(() => transport().snapshot());
  const dirty = useRef(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ y: number; height: number } | null>(null);

  useEffect(
    () =>
      transport().subscribe(() => {
        dirty.current = true;
      }),
    [],
  );
  useEffect(() => {
    const t = setInterval(() => {
      if (dirty.current) {
        dirty.current = false;
        setEvts(transport().snapshot());
      }
    }, 250);
    return () => clearInterval(t);
  }, []);
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [evts]);

  const tail = tailEvents(evts);

  if (collapsed) {
    return (
      <Box
        sx={{
          flexShrink: 0,
          display: 'flex',
          alignItems: 'center',
          gap: 1,
          px: 1,
          height: 32,
          borderTop: 1,
          borderColor: 'divider',
        }}
      >
        <Typography variant="caption">Log ({evts.length})</Typography>
        <Box sx={{ flex: 1 }} />
        <Button size="small" aria-label="Expand log" onClick={onToggleCollapse}>
          show
        </Button>
      </Box>
    );
  }

  return (
    <Box
      sx={{
        flexShrink: 0,
        display: 'flex',
        flexDirection: 'column',
        borderTop: 1,
        borderColor: 'divider',
        height,
        minHeight: 0,
      }}
    >
      <Box
        sx={{
          height: 6,
          flexShrink: 0,
          cursor: 'row-resize',
          '&:hover': { backgroundColor: 'action.hover' },
        }}
        onMouseDown={(e) => {
          dragRef.current = { y: e.clientY, height };
          const move = (m: MouseEvent) => {
            const s = dragRef.current;
            if (!s) return;
            onHeight(
              Math.max(80, Math.min(window.innerHeight * 0.6, s.height + (s.y - m.clientY))),
            );
          };
          const up = () => {
            dragRef.current = null;
            window.removeEventListener('mousemove', move);
            window.removeEventListener('mouseup', up);
          };
          window.addEventListener('mousemove', move);
          window.addEventListener('mouseup', up);
        }}
      />
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, px: 1, flexShrink: 0 }}>
        <Typography variant="caption">
          Log ({tail.length}/{evts.length}){evts.length > tail.length ? ' (last 100)' : ''}
        </Typography>
        <Box sx={{ flex: 1 }} />
        <Button
          size="small"
          onClick={() => {
            transport().clear();
            setEvts([]);
          }}
        >
          Clear
        </Button>
        <Button size="small" aria-label="Collapse log" onClick={onToggleCollapse}>
          hide
        </Button>
      </Box>
      <Box ref={scrollRef} sx={{ flex: 1, overflow: 'auto', minHeight: 0 }}>
        <List dense>
          {tail.length === 0 ? (
            <ListItem disablePadding>
              <Typography variant="body2" color="text.secondary" sx={{ px: 2, py: 1 }}>
                No events yet — compile or open a project.
              </Typography>
            </ListItem>
          ) : (
            tail.map((e, i) => {
              const jump = problemRefOf(e);
              if (jump) {
                return (
                  <ListItem key={i} disablePadding>
                    <ListItemButton
                      onClick={() => onJump(jump)}
                      title={`Jump to ${jump.path}:${jump.line}`}
                    >
                      <RowText e={e} />
                    </ListItemButton>
                  </ListItem>
                );
              }
              return (
                <ListItem key={i} disablePadding sx={{ px: 2 }}>
                  <RowText e={e} />
                </ListItem>
              );
            })
          )}
        </List>
      </Box>
    </Box>
  );
}
