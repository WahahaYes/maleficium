// useExternalRefresh.ts — reload the preview when its pdf is rewritten by
// someone other than this app (an agent's compile over MCP).
//
// Polls the pdf stamp every 1.5 s while the window is visible; the decision
// is the pure fold in externalRefresh.ts, fed the app's own compile events.
// With nothing shown yet it watches the open project's main file, so an
// agent's first compile of a project this app never built opens the preview.

import { useEffect, useRef } from 'react';
import { outputPdf, outputStamp } from '../lib/compile';
import { emit } from '../lib/events';
import { transport } from '../lib/event-transport';
import { INITIAL_REFRESH, onAppEvent, onPoll, watchTarget } from '../lib/externalRefresh';
import { emitPdf, onPdf, type PreviewDoc, type PreviewSource } from '../lib/preview-bus';

export const REFRESH_POLL_MS = 1500;

export function useExternalRefresh(main: PreviewSource | null = null) {
  const mainRef = useRef(main);
  mainRef.current = main;
  useEffect(() => {
    let doc: PreviewDoc | null = null;
    let state = INITIAL_REFRESH;
    let watched: string | null = null;
    let reported: string | null = null;
    const offPdf = onPdf((d) => {
      if (d.docKey !== doc?.docKey) state = INITIAL_REFRESH;
      doc = d;
    });
    const offBus = transport().subscribe((e) => {
      state = onAppEvent(state, e.event);
    });
    let busy = false;
    const timer = setInterval(() => {
      const t = watchTarget(doc, mainRef.current);
      if (busy || document.hidden || !t) return;
      if (t.key !== watched) {
        watched = t.key;
        state = INITIAL_REFRESH;
      }
      const shown = doc;
      busy = true;
      outputStamp(t.source.rootId, t.source.mainRel)
        .then(async (now) => {
          if (doc !== shown || watched !== t.key) return;
          const r = onPoll(state, now);
          state = r.state;
          if (!r.reload) return;
          const url = t.url ?? (await outputPdf(t.source.rootId, t.source.mainRel));
          if (!url || doc !== shown) return;
          emitPdf({ url, source: t.source, revision: null });
          emit({
            scope: 'preview',
            kind: 'info',
            actor: 'system',
            message: 'pdf rewritten outside the app: preview reloaded',
            event: { action: 'preview.external-update', pdfUrl: url },
          });
        })
        .catch((e: unknown) => {
          // Reported once per document: the next document may poll fine.
          if (reported === t.key) return;
          reported = t.key;
          emit({
            scope: 'preview',
            kind: 'warn',
            actor: 'system',
            message: 'cannot watch the pdf for outside changes: ' + String(e).slice(0, 200),
            event: { action: 'preview.stamp-failed', error: String(e).slice(0, 200) },
          });
        })
        .finally(() => {
          busy = false;
        });
    }, REFRESH_POLL_MS);
    return () => {
      offPdf();
      offBus();
      clearInterval(timer);
    };
  }, []);
}
