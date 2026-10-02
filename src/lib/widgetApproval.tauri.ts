// widgetApproval.tauri.ts — desktop IO for the Widgets panel. Reads use the
// shared operation contract; approve, revoke and auto-approval are dedicated
// desktop commands outside it, so nothing that speaks the contract (and no
// MCP tool) can reach them.

import { invoke } from '@tauri-apps/api/core';
import { request } from './core-request.tauri';
import type { WidgetsIo } from './widgetApproval';

export const desktopWidgetsIo: WidgetsIo = {
  status: (rootId, mainRel) => request('widgetsStatus', { rootId, mainRel }),
  review: (rootId, mainRel, widget) => request('widgetReview', { rootId, mainRel, widget }),
  approve: (params) => invoke('widget_approve', { params }),
  revoke: (params) => invoke('widget_revoke', { params }),
  setAutoApprove: (params) => invoke('widget_auto_approve', { params }),
};
