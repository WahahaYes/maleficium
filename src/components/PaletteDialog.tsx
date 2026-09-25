// PaletteDialog.tsx — the file finder and command palette, one box.
//
// Typing finds project files by fuzzy name; a leading `>` lists commands
// from the menus instead. Both rank with the core's fuzzy scorer. Arrow
// keys move, Enter runs, Escape closes.

import { useEffect, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import Dialog from '@mui/material/Dialog';
import List from '@mui/material/List';
import ListItemButton from '@mui/material/ListItemButton';
import TextField from '@mui/material/TextField';
import Typography from '@mui/material/Typography';
import { alpha } from '@mui/material/styles';
import type { FileMatch, Ranked } from '../lib/generated/index';
import { highlightRuns, parsePaletteInput, type PaletteCommand } from '../lib/palette';
import { typeScale } from '../lib/theme';

const ROWS = 50;

interface Row {
  key: string;
  label: string;
  positions: number[];
  detail?: string;
  run: () => void;
}

export interface PaletteDialogProps {
  open: boolean;
  /** Text the box opens with (`>` for commands). */
  initial: string;
  onClose: () => void;
  findFiles: (query: string) => Promise<FileMatch[]>;
  rank: (query: string, items: string[]) => Promise<Ranked[]>;
  commands: PaletteCommand[];
  onOpenFile: (rel: string) => void;
}

function Label({ text, positions }: { text: string; positions: number[] }) {
  return (
    <>
      {highlightRuns(text, positions).map((r, i) =>
        r.hit ? (
          <Box
            key={i}
            component="span"
            sx={{ fontWeight: 700, bgcolor: (t) => alpha(t.palette.primary.main, 0.18) }}
          >
            {r.text}
          </Box>
        ) : (
          <span key={i}>{r.text}</span>
        ),
      )}
    </>
  );
}

export default function PaletteDialog({
  open,
  initial,
  onClose,
  findFiles,
  rank,
  commands,
  onOpenFile,
}: PaletteDialogProps) {
  const [text, setText] = useState(initial);
  const [rows, setRows] = useState<Row[]>([]);
  const [active, setActive] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);
  // The command list is taken as the palette opens and held while it is up.
  const commandsRef = useRef(commands);
  if (!open) commandsRef.current = commands;

  useEffect(() => {
    if (open) setText(initial);
  }, [open, initial]);

  useEffect(() => {
    if (!open) return;
    const { mode, query } = parsePaletteInput(text);
    const mine = ++seq.current;
    const done = (r: Row[]) => {
      if (mine !== seq.current) return;
      setRows(r.slice(0, ROWS));
      setActive(0);
      setError(null);
    };
    const fail = (e: unknown) => {
      if (mine === seq.current) setError(String(e));
    };
    if (mode === 'commands') {
      const commands = commandsRef.current;
      rank(
        query,
        commands.map((c) => c.label),
      ).then(
        (ranked) =>
          done(
            ranked.map((r) => {
              const c = commands[r.index];
              return {
                // Labels repeat (two recents named `paper`); the index never does.
                key: `${c.id}:${r.index}`,
                label: c.label,
                positions: r.positions,
                detail: c.accelerator,
                run: () => void c.run(),
              };
            }),
          ),
        fail,
      );
    } else {
      findFiles(query).then(
        (files) =>
          done(
            files.map((f) => ({
              key: f.rel,
              label: f.rel,
              positions: f.positions,
              run: () => onOpenFile(f.rel),
            })),
          ),
        fail,
      );
    }
  }, [open, text, findFiles, rank, onOpenFile]);

  const runRow = (r: Row | undefined) => {
    if (!r) return;
    onClose();
    r.run();
  };

  const mode = parsePaletteInput(text).mode;
  return (
    <Dialog
      open={open}
      onClose={onClose}
      fullWidth
      maxWidth="sm"
      slotProps={{ paper: { sx: { alignSelf: 'flex-start', mt: 8 } } }}
    >
      <Box sx={{ p: 1 }}>
        <TextField
          autoFocus
          fullWidth
          size="small"
          placeholder={mode === 'commands' ? 'Run a command' : 'Go to file (type > for commands)'}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown') {
              e.preventDefault();
              setActive((a) => Math.min(a + 1, rows.length - 1));
            } else if (e.key === 'ArrowUp') {
              e.preventDefault();
              setActive((a) => Math.max(a - 1, 0));
            } else if (e.key === 'Enter') {
              e.preventDefault();
              runRow(rows[active]);
            }
          }}
          slotProps={{
            htmlInput: {
              'aria-label': mode === 'commands' ? 'Command palette' : 'Go to file',
            },
          }}
        />
        {error ? (
          <Typography variant="caption" color="error" sx={{ px: 1 }}>
            {error}
          </Typography>
        ) : rows.length === 0 ? (
          <Typography variant="caption" color="text.secondary" sx={{ display: 'block', p: 1 }}>
            {mode === 'commands' ? 'No matching commands' : 'No matching files'}
          </Typography>
        ) : null}
        <List dense disablePadding sx={{ maxHeight: 400, overflow: 'auto', mt: 0.5 }}>
          {rows.map((r, i) => (
            <ListItemButton
              key={r.key}
              selected={i === active}
              onClick={() => runRow(r)}
              onMouseMove={() => setActive(i)}
              sx={{ py: 0.25, gap: 1 }}
            >
              <Typography
                variant="body2"
                noWrap
                sx={{ flex: 1, minWidth: 0, fontSize: typeScale.dense }}
              >
                <Label text={r.label} positions={r.positions} />
              </Typography>
              {r.detail ? (
                <Typography variant="caption" color="text.secondary">
                  {r.detail}
                </Typography>
              ) : null}
            </ListItemButton>
          ))}
        </List>
      </Box>
    </Dialog>
  );
}
