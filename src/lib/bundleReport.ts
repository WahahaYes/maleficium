// bundleReport.ts — the export warnings as the report dialog shows them.
//
// Kinds grow on the Rust side (size, fold, convert, join, figure ...), so the
// label is derived from the kind string and nothing here lists them: a new
// kind needs no change in this file or the dialog.

export interface ReportWarning {
  kind: string;
  message: string;
}

export interface ReportGroup {
  kind: string;
  /** The kind as a sentence-case label: `size-cap` becomes `Size cap`. */
  label: string;
  messages: string[];
}

export interface BundleReport {
  /** What was written, e.g. `single-file bundle`. */
  profile: string;
  path: string;
  bytes: number;
  groups: ReportGroup[];
  /** Total warnings across every group. */
  count: number;
}

export function kindLabel(kind: string): string {
  const words = kind
    .replace(/[-_.]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .toLowerCase();
  return words ? words[0].toUpperCase() + words.slice(1) : 'Other';
}

/** Group by kind, in order of first appearance, keeping each message. */
export function groupWarnings(warnings: readonly ReportWarning[]): ReportGroup[] {
  const groups = new Map<string, ReportGroup>();
  for (const w of warnings) {
    let g = groups.get(w.kind);
    if (!g) {
      g = { kind: w.kind, label: kindLabel(w.kind), messages: [] };
      groups.set(w.kind, g);
    }
    g.messages.push(w.message);
  }
  return [...groups.values()];
}

export function makeReport(
  profile: string,
  exported: { path: string; bytes: number; warnings: readonly ReportWarning[] },
): BundleReport {
  return {
    profile,
    path: exported.path,
    bytes: exported.bytes,
    groups: groupWarnings(exported.warnings),
    count: exported.warnings.length,
  };
}
