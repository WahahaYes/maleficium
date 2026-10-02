// useWidgetApproval — binds the Widgets panel model and the approval prompt
// queue to the open project and its main file, and keeps them current from
// the app bus.

import { useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import { desktopWidgetsIo } from '../lib/widgetApproval.tauri';
import { transport } from '../lib/event-transport';
import {
  createPromptQueue,
  createWidgetsModel,
  promptFromEvent,
  widgetEventFor,
  type PromptQueue,
  type WidgetsModel,
} from '../lib/widgetApproval';

export function useWidgetApproval(deps: { projectId: string | null; mainRel: string | null }) {
  const { projectId, mainRel } = deps;
  const [panelOpen, setPanelOpen] = useState(false);
  const target = projectId && mainRel ? { projectId, mainRel } : null;
  const model: WidgetsModel | null = useMemo(
    () => (target ? createWidgetsModel(desktopWidgetsIo, target.projectId, target.mainRel) : null),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [target?.projectId, target?.mainRel],
  );
  const prompts: PromptQueue | null = useMemo(
    () => (target ? createPromptQueue(desktopWidgetsIo, target.projectId, target.mainRel) : null),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [target?.projectId, target?.mainRel],
  );

  useEffect(() => {
    if (!model || !prompts || !projectId) return;
    return transport().subscribe((e) => {
      const p = promptFromEvent(e, projectId);
      if (p) prompts.enqueue(p);
      if (widgetEventFor(e, projectId) && panelOpen) void model.refresh();
    });
  }, [model, prompts, projectId, panelOpen]);

  useEffect(() => {
    if (panelOpen && model) void model.refresh();
  }, [panelOpen, model]);

  const state = useSyncExternalStore(
    (cb) => (model ? model.subscribe(cb) : () => {}),
    () => (model ? model.get() : null),
  );
  const promptState = useSyncExternalStore(
    (cb) => (prompts ? prompts.subscribe(cb) : () => {}),
    () => (prompts ? `${prompts.current()?.key ?? ''}|${prompts.failure() ?? ''}` : ''),
  );
  void promptState;

  return {
    panelOpen,
    openPanel: () => setPanelOpen(true),
    closePanel: () => setPanelOpen(false),
    model,
    state,
    prompts,
    /** Entry point for the poster renderer (ip.43). */
    onApprovalRequired: prompts ? prompts.onApprovalRequired : null,
  };
}
