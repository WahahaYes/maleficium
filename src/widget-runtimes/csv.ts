// A small RFC 4180 CSV/TSV reader for the table and chart runtimes. It runs
// inside the sandboxed widget frame, so it never throws on hostile input:
// every loop is bounded by the input length.

export interface Parsed {
  header: string[];
  rows: string[][];
}

/** Picks the delimiter that splits the first line most often, ignoring quoted text. */
export function detectDelimiter(text: string): string {
  const counts: Record<string, number> = { ',': 0, '\t': 0, ';': 0 };
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '"') quoted = !quoted;
    else if (!quoted && (c === '\n' || c === '\r')) break;
    else if (!quoted && c in counts) counts[c]++;
  }
  let best = ',';
  for (const d of ['\t', ';']) if (counts[d] > counts[best]) best = d;
  return best;
}

/** Parses CSV text. The first record is the header; ragged rows are padded or cut to its width. */
export function parseCsv(input: string, delimiter?: string): Parsed {
  const text = input.charCodeAt(0) === 0xfeff ? input.slice(1) : input;
  const delim = delimiter ?? detectDelimiter(text);
  const records: string[][] = [];
  let field = '';
  let record: string[] = [];
  let quoted = false;
  let touched = false; // the current record holds anything, even an empty quoted field
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          field += '"';
          i++;
        } else quoted = false;
      } else field += c;
    } else if (c === '"' && field === '') {
      quoted = true;
      touched = true;
    } else if (c === delim) {
      record.push(field);
      field = '';
      touched = true;
    } else if (c === '\n' || c === '\r') {
      if (c === '\r' && text[i + 1] === '\n') i++;
      if (touched || field !== '') {
        record.push(field);
        records.push(record);
      }
      record = [];
      field = '';
      touched = false;
    } else field += c;
  }
  if (touched || field !== '') {
    record.push(field);
    records.push(record);
  }
  if (records.length === 0) return { header: [], rows: [] };
  const header = records[0].map((h) => h.trim());
  const width = header.length;
  const rows = records.slice(1).map((r) => {
    const cells = r.slice(0, width);
    while (cells.length < width) cells.push('');
    return cells;
  });
  return { header, rows };
}

const NUMBER = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/;

/** The number a cell holds, or null when it is empty or not a plain decimal. */
export function toNumber(raw: string): number | null {
  const t = raw.trim();
  if (!NUMBER.test(t)) return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
}
