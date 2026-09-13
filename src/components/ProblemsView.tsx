// ProblemsView.tsx — presentational clickable-rows list.
//
// Used only by tests/stories now that the app renders problems inline in the
// unified `LogStream`. Kept (not deleted) so `Problems.test.ts` semantics stay
// covered alongside `lib/parseLog.ts` units.

import { List, ListItem, ListItemButton, ListItemText } from '@mui/material';
import { parseLog, type ParsedLine } from '../lib/parseLog';

export default function ProblemsView({ logText, root, base, onJump, entries: entriesProp, totalCount }: {
  logText: string;
  root: string;
  base: string;
  onJump: (absPath: string, line: number) => void;
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
