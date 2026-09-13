// BottomPanel.tsx — tabbed Problems/Log/Terminal dock.
//
// Growth cap: Problems slice(0,100); events bus cap 500 + 4Hz flush + render
// slice 100 (01 `5012719` preserved); resizable vertical split via PaneSplitter
// CSS; popout floats same events reference (stub); collapsed → Drawer temporary.

import { useEffect } from 'react';
import Paper from '@mui/material/Paper';
import Tabs from '@mui/material/Tabs';
import Tab from '@mui/material/Tab';
import Box from '@mui/material/Box';
import Drawer from '@mui/material/Drawer';
import Problems from './Problems';
import EventLog from './EventLog';
import type { BusEvent } from '../lib/events';

export type BottomTab = 'problems' | 'log' | 'terminal';

export default function BottomPanel({ tab, onTab, problems, events, terminalVisible, height, onHeight, popout, collapsed, mainFile }: {
  tab: BottomTab;
  onTab: (t: BottomTab) => void;
  problems: { logText: string; root: string; base: string; onJump: (p: string, l: number) => void };
  events: BusEvent[];
  terminalVisible: boolean;
  height: number;
  onHeight: (h: number) => void;
  popout?: boolean;
  collapsed?: boolean;
  mainFile?: string | null;
}) {
  void events;
  const startDragY: { y: number | null } = { y: null };

  useEffect(() => {
    if (popout) {
      // eslint-disable-next-line no-console
      console.log('popout: bottom panel (stub — same events reference)');
    }
  }, [popout]);

  const visible = problems.logText
    ? problems.logText.split('\n').length > 100
      ? problems.logText.split('\n').slice(0, 100).join('\n')
      : problems.logText
    : problems.logText;

  const body = (
    <Paper elevation={2} sx={{ height, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
      <Box
        sx={{ height: 6, cursor: 'row-resize', '&:hover': { backgroundColor: 'action.hover' } }}
        onMouseDown={(e) => {
          startDragY.y = e.clientY;
          const move = (m: MouseEvent) => {
            if (startDragY.y == null) return;
            onHeight(Math.max(80, Math.min(window.innerHeight * 0.6, height + (startDragY.y - m.clientY))));
          };
          const up = () => {
            startDragY.y = null;
            window.removeEventListener('mousemove', move);
            window.removeEventListener('mouseup', up);
          };
          window.addEventListener('mousemove', move);
          window.addEventListener('mouseup', up);
        }}
      />
      <Tabs value={tab} onChange={(_, v) => onTab(v)} variant="fullWidth" sx={{ minHeight: 36 }}>
        <Tab value="problems" label="Problems" sx={{ minHeight: 36 }} />
        <Tab value="log" label="Log" sx={{ minHeight: 36 }} />
        {terminalVisible ? <Tab value="terminal" label="Terminal" sx={{ minHeight: 36 }} /> : null}
      </Tabs>
      <Box sx={{ flex: 1, overflow: 'auto', display: 'flex' }}>
        {tab === 'problems' ? (
          <Box sx={{ flex: 1, overflow: 'auto' }}>
            <Problems logText={visible} root={problems.root} base={problems.base} onJump={problems.onJump} />
          </Box>
        ) : null}
        {tab === 'log' ? (
          <Box sx={{ flex: 1, overflow: 'auto' }}>
            <EventLog />
          </Box>
        ) : null}
        {tab === 'terminal' && terminalVisible ? (
          <Box sx={{ flex: 1, overflow: 'auto', p: 1, fontFamily: 'monospace', fontSize: 12 }}>
            terminal (shrink — cwd: {problems.base}{mainFile ? ` · main: ${mainFile}` : ''})
          </Box>
        ) : null}
      </Box>
    </Paper>
  );

  if (collapsed) {
    return (
      <Drawer anchor="bottom" variant="temporary" open={false} onClose={() => {}}>
        {body}
      </Drawer>
    );
  }
  return body;
}
