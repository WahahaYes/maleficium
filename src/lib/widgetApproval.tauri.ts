// widgetApproval.tauri.ts — desktop IO for the Widgets panel. Reads use the
// shared operation contract; approve, revoke and auto-approval are dedicated
// desktop commands outside it, so nothing that speaks the contract (and no
// MCP tool) can reach them. Runtime review and decisions are dedicated
// `runtime_*` commands for the same reason.

import { invoke } from '@tauri-apps/api/core';
import { request } from './core-request.tauri';
import type { WidgetsIo } from './widgetApproval';
import type { RuntimeDecisionParams, WidgetApprovalStatus, WidgetReview } from './generated/api';

export const desktopWidgetsIo: WidgetsIo = {
  status: (rootId, mainRel) => request('widgetsStatus', { rootId, mainRel }),
  review: (rootId, mainRel, widget) => request('widgetReview', { rootId, mainRel, widget }),
  approve: (params) => invoke('widget_approve', { params }),
  revoke: (params) => invoke('widget_revoke', { params }),
  setAutoApprove: (params) => invoke('widget_auto_approve', { params }),
  reviewRuntime: (rootId, mainRel, runtime): Promise<WidgetReview> =>
    invoke('runtime_review', { params: { rootId, mainRel, runtime } }),
  decideRuntime: (params: RuntimeDecisionParams): Promise<WidgetApprovalStatus> =>
    invoke('runtime_decide', { params }),
};
