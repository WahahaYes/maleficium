import { List, ListItem, ListItemButton, ListItemText } from '@mui/material';

export interface ParsedLine {
  file: string;
  line: number;
  msg: string;
  clickable: boolean;
}

function joinPath(base: string, file: string): string {
  if (!base.endsWith('/')) base += '/';
  return base + file;
}

function resolvePath(p: string): string {
  const parts = p.split('/');
  const out: string[] = [];
  for (const part of parts) {
    if (part === '..') out.pop();
    else if (part !== '.' && part !== '') out.push(part);
  }
  return '/' + out.join('/');
}

export function parseLog(logText: string, root: string, base: string): ParsedLine[] {
  const lines: ParsedLine[] = [];
  const re = /(?:^|\s)(\.\/)?([\w\-./]+\.tex):(\d+):?\s*(.*)/;
  for (const line of logText.split('\n')) {
    const m = line.match(re);
    if (m) {
      let file = m[2];
      file = file.startsWith('/') ? resolvePath(file) : resolvePath(joinPath(base, file));
      const clickable = file.startsWith(root);
      lines.push({ file, line: parseInt(m[3], 10), msg: m[4], clickable });
    }
  }
  return lines;
}

export default function Problems({ logText, root, base, onJump, entries: entriesProp, totalCount }: {
  logText: string;
  root: string;
  base: string;
  onJump: (absPath: string, line: number) => void;
  /** Pre-parsed + pre-capped entries (BottomPanel parses once, then caps). */
  entries?: ParsedLine[];
  totalCount?: number;
}) {
  const lines = entriesProp ?? parseLog(logText, root, base);
  const total = totalCount ?? lines.length;
  if (lines.length === 0) {
    return (
      <List dense>
        <ListItem disablePadding>
          <ListItemText primary="No problems" secondary="Compile output produced no file:line errors." />
        </ListItem>
      </List>
    );
  }
  return (
    <List dense>
      {lines.map((l, i) => (
        <ListItem key={i} disablePadding>
          <ListItemButton disabled={!l.clickable} onClick={() => l.clickable && onJump(l.file, l.line)}>
            <ListItemText
              primary={`${l.file}:${l.line} ${l.msg}`}
              secondary={l.clickable ? undefined : 'Outside workspace — not clickable'}
            />
          </ListItemButton>
        </ListItem>
      ))}
      {total > lines.length ? (
        <ListItem disablePadding>
          <ListItemText primary={`… ${total - lines.length} more (showing 100)`} />
        </ListItem>
      ) : null}
    </List>
  );
}