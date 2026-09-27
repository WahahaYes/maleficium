// useExport.ts — export the shown pdf, or the project's sources as a zip, to
// a place the user picks outside the project.

import { dialog } from '../lib/fs-provider';
import { exportPdf, exportZip } from '../lib/compile';
import { emit } from '../lib/events';
import type { ExportKind } from '../lib/generated/events';
import type { PreviewSource, SessionRoot } from '../lib/preview-bus';
import { baseName } from '../lib/paths';

/** The last path segment without a `.tex` suffix, for default file names. */
function stem(path: string): string {
  const base = baseName(path);
  return base.endsWith('.tex') ? base.slice(0, -4) : base;
}

export function useExport(deps: { pdf: PreviewSource | null; project: SessionRoot | null }) {
  const report = (kind: ExportKind, r: Promise<{ path: string; bytes: number }>) =>
    r.then(
      (e) =>
        emit({
          scope: 'app',
          kind: 'success',
          actor: 'user',
          message: `exported ${kind} to ${e.path} (${e.bytes} bytes)`,
          event: { action: 'export.done', kind, path: e.path, bytes: e.bytes },
        }),
      (err: unknown) =>
        emit({
          scope: 'app',
          kind: 'error',
          actor: 'user',
          message: `export ${kind} failed: ` + String(err).slice(0, 200),
          event: { action: 'export.failed', kind, error: String(err).slice(0, 200) },
        }),
    );

  async function exportPdfAs() {
    const src = deps.pdf;
    if (!src) {
      await report('pdf', Promise.reject(new Error('no compiled pdf to export: compile first')));
      return;
    }
    const dest = await dialog().saveFile({
      title: 'Export PDF',
      defaultPath: stem(src.mainRel) + '.pdf',
      filters: [{ name: 'PDF', extensions: ['pdf'] }],
    });
    if (dest) await report('pdf', exportPdf(src.rootId, src.mainRel, dest));
  }

  async function exportZipAs() {
    const p = deps.project;
    if (!p) {
      await report('zip', Promise.reject(new Error('open a project to export it')));
      return;
    }
    const dest = await dialog().saveFile({
      title: 'Export Project as Zip',
      defaultPath: stem(p.path) + '.zip',
      filters: [{ name: 'Zip archive', extensions: ['zip'] }],
    });
    if (dest) await report('zip', exportZip(p.rootId, dest));
  }

  return { exportPdfAs, exportZipAs };
}
