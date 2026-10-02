// surface.tauri.ts — the desktop widget surface: the app's widget host over
// Tauri commands, its events on `widget-event`. Every event also goes on
// the app bus, so the event log records it.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { WidgetSession } from '../generated/api';
import type { BusEvent } from '../generated/events';
import { transport } from '../event-transport';
import type { WidgetSurface } from './surface';

const subs = new Set<(e: BusEvent) => void>();
let listening = false;

function ensureListening() {
  if (listening) return;
  listening = true;
  void listen<BusEvent>('widget-event', (e) => {
    transport().publish(e.payload);
    subs.forEach((cb) => cb(e.payload));
  });
}

const fire = (cmd: string, args: Record<string, unknown> = {}) => {
  void invoke(cmd, args).catch(() => {
    // A dropped placement or focus change is replaced by the next one.
  });
};

export const desktopWidgets: WidgetSurface = {
  open(rootId, mainRel, dark) {
    ensureListening();
    return invoke<WidgetSession>('widgets_open', { rootId, mainRel, dark });
  },
  approve(rootId, mainRel, scope, granted) {
    ensureListening();
    return invoke<WidgetSession>('widgets_approve', { rootId, mainRel, scope, granted });
  },
  view: (items) => fire('widgets_view', { items }),
  activate: (id, on) => fire('widgets_activate', { id, on }),
  deactivateAll: () => fire('widgets_deactivate_all'),
  setHidden: (hidden) => fire('widgets_hidden', { hidden }),
  setTheme: (dark) => fire('widgets_theme', { dark }),
  reload: (id) => fire('widgets_reload', { id }),
  close: () => fire('widgets_close'),
  onEvent(cb) {
    ensureListening();
    subs.add(cb);
    return () => {
      subs.delete(cb);
    };
  },
};
