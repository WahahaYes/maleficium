// parseLog.ts — engine-log → clickable file:line entries (rebase-once).
//
// Pure function (tested). The producing command's dir is `base`; entries
// resolving outside workspace `root` are shown, not clickable. Consumed by
// `App.tsx` (Problems click-to-jump) and the `LogStream` event stream.

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
