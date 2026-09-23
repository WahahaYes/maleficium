// useExternalRefresh.ts — reload the preview when its pdf is rewritten by
// someone other than this app (an agent's compile over MCP).
//
// Polls the pdf stamp every 1.5 s while the window is visible; the decision
// is the pure fold in externalRefresh.ts, fed the app's own compile events.

import { useEffect } from 'react';
import { outputStamp } from '../lib/compile';
import { emit } from '../lib/events';
import { transport } from '../lib/event-transport';
import { INITIAL_REFRESH, onAppEvent, onPoll } from '../lib/externalRefresh';
import { emitPdf, onPdf, type PreviewDoc } from '../lib/preview-bus';

export const REFRESH_POLL_MS = 1500;

export function useExternalRefresh() {
  useEffect(() => {
    let doc: PreviewDoc | null = null;
    let state = INITIAL_REFRESH;
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
      const d = doc;
      if (busy || document.hidden || !d?.source || !d.url) return;
      busy = true;
      outputStamp(d.source.rootId, d.source.mainRel)
        .then((now) => {
          if (d !== doc) return;
          const r = onPoll(state, now);
          state = r.state;
          if (!r.reload || !d.url) return;
          emitPdf({ url: d.url, source: d.source, revision: null });
          emit({
            scope: 'preview',
            kind: 'info',
            actor: 'system',
            message: 'pdf rewritten outside the app: preview reloaded',
            event: { action: 'preview.external-update', pdfUrl: d.url },
          });
        })
        .catch((e: unknown) => {
          // Reported once per document: the next document may poll fine.
          if (reported === d.docKey) return;
          reported = d.docKey;
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
