// useWidgetSession.ts — the shown paper's interactive widgets: their rects,
// the project's approvals, and each widget's live state. Opens the session
// for every compile of the shown main file and closes it when the paper
// goes; approvals apply at once, without a reload.

import { useCallback, useEffect, useState } from 'react';
import { request } from '../lib/core-request.tauri';
import { emit } from '../lib/events';
import type { Widget, WidgetSession } from '../lib/generated/api';
import type { WidgetApprovalScope } from '../lib/generated/events';
import type { PreviewSource } from '../lib/preview-bus';
import { initialStates, reduce, type WidgetStates } from '../lib/widgets/state';
import { widgetSurface, type WidgetSurface } from '../lib/widgets/surface';

export interface WidgetsModel {
  surface: WidgetSurface | null;
  widgets: Widget[];
  session: WidgetSession | null;
  states: WidgetStates;
  approve: (scope: WidgetApprovalScope, granted: boolean) => void;
  reload: (id: string) => void;
}

/** No widgets and no host: what a preview without a paper session uses. */
export const NO_WIDGETS: WidgetsModel = {
  surface: null,
  widgets: [],
  session: null,
  states: {},
  approve: () => {},
  reload: () => {},
};

export function useWidgetSession(
  source: PreviewSource | null,
  stamp: number,
  dark: boolean,
): WidgetsModel {
  const surface = widgetSurface();
  const [widgets, setWidgets] = useState<Widget[]>([]);
  const [session, setSession] = useState<WidgetSession | null>(null);
  const [states, setStates] = useState<WidgetStates>({});
  const rootId = source?.rootId ?? null;
  const mainRel = source?.mainRel ?? null;

  const adopt = useCallback((s: WidgetSession) => {
    setSession(s);
    setStates(initialStates(s.entries, s.unavailable.length > 0));
  }, []);

  useEffect(() => {
    if (!surface || !rootId || !mainRel) {
      setWidgets([]);
      setSession(null);
      surface?.close();
      return;
    }
    let live = true;
    request('widgets', { rootId, mainRel })
      .then(async (list) => {
        if (!live) return;
        setWidgets(list.widgets);
        if (list.widgets.length === 0) {
          setSession(null);
          surface.close();
          return;
        }
        const s = await surface.open(rootId, mainRel, dark);
        if (live) adopt(s);
      })
      .catch(() => {
        // Not compiled yet, or a stale sidecar: no widgets until it is.
        if (!live) return;
        setWidgets([]);
        setSession(null);
        surface.close();
      });
    return () => {
      live = false;
    };
    // `dark` reaches a running session through setTheme below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [surface, rootId, mainRel, stamp, adopt]);

  useEffect(() => () => surface?.close(), [surface]);

  useEffect(() => {
    surface?.setTheme(dark);
  }, [surface, dark]);

  useEffect(() => surface?.onEvent((e) => setStates((s) => reduce(s, e))), [surface]);

  const approve = useCallback(
    (scope: WidgetApprovalScope, granted: boolean) => {
      if (!surface || !rootId || !mainRel) return;
      surface
        .approve(rootId, mainRel, scope, granted)
        .then(adopt)
        .catch((e: unknown) =>
          emit({
            scope: 'preview',
            kind: 'error',
            actor: 'user',
            message: 'widget approval failed',
            event: { action: 'widgets.failed', main: mainRel, error: String(e).slice(0, 200) },
          }),
        );
    },
    [surface, rootId, mainRel, adopt],
  );

  const reload = useCallback(
    (id: string) => {
      // Back in the running set, so the next placement offers it a slot.
      setStates((s) => (s[id] ? { ...s, [id]: { phase: 'idle', active: false } } : s));
      surface?.reload(id);
    },
    [surface],
  );

  return { surface, widgets, session, states, approve, reload };
}
