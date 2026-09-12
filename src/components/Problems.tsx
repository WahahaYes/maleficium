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
    else if (part !== '.') out.push(part);
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
      if (m[1] === './') file = resolvePath(joinPath(base, file));
      else file = resolvePath(file);
      const clickable = file.startsWith(root);
      lines.push({ file, line: parseInt(m[3], 10), msg: m[4], clickable });
    }
  }
  return lines;
}

export default function Problems({ logText, root, base, onJump }: {
  logText: string;
  root: string;
  base: string;
  onJump: (absPath: string, line: number) => void;
}) {
  const lines = parseLog(logText, root, base);
  return (
    <List dense>
      {lines.map((l, i) => (
        <ListItem key={i} disablePadding>
          <ListItemButton onClick={() => l.clickable && onJump(l.file, l.line)}>
            <ListItemText primary={`${l.file}:${l.line} ${l.msg}`} />
          </ListItemButton>
        </ListItem>
      ))}
    </List>
  );
}