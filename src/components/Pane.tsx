// Pane.tsx — split wrapper (CSS split, no custom widget library).
//
// Growth cap: ratios are scalars (editorRatio/previewRatio 0..1); resizing never
// grows content layers. (Popout/collapsed stubs cut — dead buttons lie.)

import Box from '@mui/material/Box';
import Divider from '@mui/material/Divider';
import { type ReactNode, useCallback, useRef } from 'react';

export interface PaneProps {
  children: ReactNode;
  ratio: number;
  onRatio: (r: number) => void;
  minRatio?: number;
  maxRatio?: number;
  label: string;
}

export function PaneSplitter({
  onDrag,
  onKeyResize,
  label,
}: {
  onDrag: (dx: number) => void;
  onKeyResize?: (dir: 1 | -1) => void;
  label?: string;
}) {
  const startX = useRef(0);
  return (
    <Divider
      orientation="vertical"
      flexItem
      role="separator"
      aria-orientation="vertical"
      aria-label={label ?? 'Resize panes'}
      tabIndex={0}
      onKeyDown={(e) => {
        if (!onKeyResize) return;
        if (e.key === 'ArrowLeft') {
          e.preventDefault();
          onKeyResize(-1);
        } else if (e.key === 'ArrowRight') {
          e.preventDefault();
          onKeyResize(1);
        }
      }}
      sx={{
        cursor: 'col-resize',
        width: 8,
        '&:hover': { backgroundColor: 'action.hover' },
        '&:focus-visible': { backgroundColor: 'action.selected' },
      }}
      onMouseDown={(e) => {
        startX.current = e.clientX;
        const move = (m: MouseEvent) => {
          onDrag(m.clientX - startX.current);
          startX.current = m.clientX;
        };
        const up = () => {
          window.removeEventListener('mousemove', move);
          window.removeEventListener('mouseup', up);
        };
        window.addEventListener('mousemove', move);
        window.addEventListener('mouseup', up);
      }}
    />
  );
}

export default function Pane({
  children,
  ratio,
  onRatio,
  minRatio = 0.2,
  maxRatio = 0.8,
  label,
}: PaneProps) {
  const handleDrag = useCallback(
    (dx: number) => {
      const w = window.innerWidth || 1000;
      onRatio(Math.min(maxRatio, Math.max(minRatio, ratio + dx / w)));
    },
    [ratio, onRatio, minRatio, maxRatio],
  );

  return (
    <Box
      sx={{
        flex: ratio,
        minWidth: 120,
        overflow: 'auto',
        display: 'flex',
        flexDirection: 'column',
      }}
      data-pane={label}
    >
      {children}
      <Box sx={{ display: 'none' }} data-splitter={label}>
        {/* splitter handle rendered by parent between panes */}
        <PaneSplitter onDrag={handleDrag} />
      </Box>
    </Box>
  );
}
