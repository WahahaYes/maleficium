import { useCallback, useEffect, useMemo } from 'react';
import { anchorForLine, shouldRefreshArticle } from '../lib/article';
import { transport } from '../lib/event-transport';
import { emit, eventOf } from '../lib/events';
import { sourceFor } from '../lib/preview-bus';
import { approvalBannerText, pendingRuntimes, pendingWidgets } from '../lib/widgets.view';
import { useArticle } from './useArticle';
import type { useWidgetApproval } from './useWidgetApproval';

type Widgets = Pick<ReturnType<typeof useWidgetApproval>, 'state' | 'runtimeState' | 'openPanel'>;

// The Article tab kept in step with the project: reloads after a newer
// compile, follows the caret's section, names held widgets, and
// re-exports when an approval settles.
export function useArticleSync({
  mainFile,
  root,
  projectId,
  previewStamp,
  compiling,
  currentLine,
  sections,
  widgets,
}: {
  mainFile: string | null;
  root: string | null;
  projectId: string | null;
  /** The preview's stamp; a finished compile bumps it. */
  previewStamp: number | null;
  compiling: boolean;
  currentLine: number;
  /** The active buffer's outline sections. */
  sections: { line: number; title: string }[];
  widgets: Widgets;
}) {
  const mainDoc = useMemo(
    () =>
      mainFile && root && projectId
        ? sourceFor(mainFile, [{ rootId: projectId, path: root }])
        : null,
    [mainFile, root, projectId],
  );
  // The Article tab's bytes: loaded on demand, reloaded by hand.
  const article = useArticle();
  const { src, stamp, load, anchors, reload } = article;
  // Auto-refresh: when the article shows bytes older than the preview,
  // reload it. Never loads on its own: the first visit stays manual (see
  // shouldRefreshArticle).
  useEffect(() => {
    if (!mainDoc) return;
    if (shouldRefreshArticle(src, stamp, previewStamp, compiling, true)) {
      void load(mainDoc.rootId, mainDoc.mainRel, previewStamp ?? 0);
    }
  }, [mainDoc, previewStamp, compiling, src, stamp, load]);
  const loadNow = useCallback(() => {
    if (mainDoc) void load(mainDoc.rootId, mainDoc.mainRel, previewStamp ?? 0);
  }, [mainDoc, load, previewStamp]);

  // Forward-only sync: the caret line's section maps to the article anchor
  // the frame scrolls to. The article names the same headings the outline
  // does, with latexml numbers attached.
  const anchor = useMemo(
    () => anchorForLine(currentLine, sections, anchors),
    [currentLine, sections, anchors],
  );
  const onSync = useCallback((anchor: string) => {
    emit({
      scope: 'preview',
      kind: 'info',
      actor: 'user',
      message: `article sync → ${anchor}`,
      event: { action: 'article.sync', anchor },
    });
  }, []);

  // The banner names the held widgets and runtimes from the same listing
  // the Widgets panel shows; approving there clears it.
  const approvalText = useMemo(
    () =>
      approvalBannerText(
        pendingWidgets(widgets.state?.status ?? null),
        pendingRuntimes(widgets.runtimeState?.status ?? null),
      ),
    [widgets.state, widgets.runtimeState],
  );
  // Approve round-trip: the bytes' Approve button opens the Widgets panel
  // (approval wants the panel's review, never a blind click-through); the
  // approval settled there re-exports the article.
  const { openPanel } = widgets;
  const onApproveRequest = useCallback(
    (widget: string) => {
      emit({
        scope: 'preview',
        kind: 'info',
        actor: 'user',
        message: `article asks approval for ${widget}`,
        event: { action: 'article.approve-request', widget },
      });
      openPanel();
    },
    [openPanel],
  );
  useEffect(() => {
    if (!projectId) return;
    return transport().subscribe((e) => {
      const acted = eventOf(e, 'widget.approved') ?? eventOf(e, 'widget.revoked');
      if (acted && acted.rootId === projectId) reload();
    });
  }, [projectId, reload]);

  return { article, mainDoc, loadNow, anchor, onSync, approvalText, onApproveRequest };
}
