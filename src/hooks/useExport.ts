// useExport.ts — export the shown pdf, or the project's sources as a zip, to
// a place the user picks outside the project.

import { useState } from 'react';
import { dialog } from '../lib/fs-provider';
import { exportBundle, exportCancel, exportPdf, exportZip, previewInBrowser } from '../lib/compile';
import { emit } from '../lib/events';
import type { BundleProfile, ExportKind } from '../lib/generated/events';
import type { PreviewSource, SessionRoot } from '../lib/preview-bus';
import { makeReport, type BundleReport } from '../lib/bundleReport';
import { baseName, joinPath } from '../lib/paths';
import { cancelRun, canCancel, isCancelled, startRun, type ExportRun } from '../lib/exportProgress';

/** The last path segment without a `.tex` suffix, for default file names. */
function stem(path: string): string {
  const base = baseName(path);
  return base.endsWith('.tex') ? base.slice(0, -4) : base;
}

export function useExport(deps: { pdf: PreviewSource | null; project: SessionRoot | null }) {
  const [bundleReport, setBundleReport] = useState<BundleReport | null>(null);
  const [run, setRun] = useState<ExportRun | null>(null);

  async function cancelExport() {
    if (!canCancel(run)) return;
    setRun(cancelRun);
    try {
      await exportCancel();
    } catch {
      // Nothing was converting any more: the export is finishing on its own.
    }
  }
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

  const cancelled = (main: string, profile: BundleProfile) =>
    emit({
      scope: 'app',
      kind: 'warn',
      actor: 'user',
      message: 'paper bundle export cancelled; nothing was written',
      event: { action: 'bundle.failed', main, profile, error: 'cancelled' },
    });

  /** The folder profiles ask for a parent folder and make `<name>-bundle` in it. */
  async function exportBundleAs(profile: BundleProfile) {
    const src = deps.pdf;
    const fail = (error: string) =>
      emit({
        scope: 'app',
        kind: 'error',
        actor: 'user',
        message: `paper bundle export failed: ${error}`.slice(0, 240),
        event: { action: 'bundle.failed', main: src?.mainRel ?? '', profile, error },
      });
    if (!src) {
      fail('no compiled pdf to export: compile first');
      return;
    }
    const single = profile === 'single-file';
    const picked = single
      ? await dialog().saveFile({
          title: 'Export Paper Bundle as One File',
          defaultPath: stem(src.mainRel) + '.html',
          filters: [{ name: 'HTML', extensions: ['html'] }],
        })
      : await dialog().openDirectory({ title: 'Export Paper Bundle: choose a parent folder' });
    if (!picked) return;
    const dest = single ? picked : joinPath(picked, stem(src.mainRel) + '-bundle');
    // No approval gate: approval decides what runs inside the app, and an
    // export ships every widget live under the reader's sandbox (a denied
    // or invalid runtime as its poster, with a warning).
    setRun(startRun(`${profile} bundle`));
    try {
      const r = await exportBundle(src.rootId, src.mainRel, dest, profile);
      const notes = r.warnings.map((w) => w.message).join('; ');
      if (r.warnings.length > 0) setBundleReport(makeReport(profile, r));
      emit({
        scope: 'app',
        kind: r.warnings.length > 0 ? 'warn' : 'success',
        actor: 'user',
        message:
          `exported a ${profile} bundle to ${r.path} (${r.bytes} bytes)` +
          (notes ? `: ${notes}` : '').slice(0, 400),
        event: {
          action: 'bundle.exported',
          main: src.mainRel,
          profile,
          path: r.path,
          bytes: r.bytes,
          widgets: r.widgets,
          warnings: r.warnings.length,
        },
      });
    } catch (err) {
      if (isCancelled(err)) cancelled(src.mainRel, profile);
      else fail(String(err).slice(0, 300));
    } finally {
      setRun(null);
    }
  }

  /** One command: export the single-file bundle to a scratch folder and open it. */
  async function previewBundle() {
    const src = deps.pdf;
    const profile: BundleProfile = 'single-file';
    if (!src) {
      emit({
        scope: 'app',
        kind: 'error',
        actor: 'user',
        message: 'preview failed: no compiled pdf to preview: compile first',
        event: { action: 'bundle.failed', main: '', profile, error: 'no compiled pdf' },
      });
      return;
    }
    setRun(startRun('browser preview'));
    try {
      const r = await previewInBrowser(src.rootId, src.mainRel);
      if (r.warnings.length > 0) setBundleReport(makeReport(profile, r));
      emit({
        scope: 'app',
        kind: r.warnings.length > 0 ? 'warn' : 'success',
        actor: 'user',
        message: `opened a preview of ${src.mainRel} in the browser (${r.path})`.slice(0, 400),
        event: {
          action: 'bundle.exported',
          main: src.mainRel,
          profile,
          path: r.path,
          bytes: r.bytes,
          widgets: r.widgets,
          warnings: r.warnings.length,
        },
      });
    } catch (err) {
      if (isCancelled(err)) {
        cancelled(src.mainRel, profile);
        return;
      }
      const error = String(err).slice(0, 300);
      emit({
        scope: 'app',
        kind: 'error',
        actor: 'user',
        message: `preview failed: ${error}`.slice(0, 240),
        event: { action: 'bundle.failed', main: src.mainRel, profile, error },
      });
    } finally {
      setRun(null);
    }
  }

  return {
    exportPdfAs,
    exportZipAs,
    exportBundleAs,
    previewBundle,
    bundleReport,
    run,
    cancelExport,
    closeBundleReport: () => setBundleReport(null),
  };
}
