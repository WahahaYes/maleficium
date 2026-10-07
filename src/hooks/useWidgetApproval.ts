// useWidgetApproval — binds the Widgets panel model and the approval prompt
// queue to the open project and its main file, and keeps them current from
// the app bus. The listing stays fresh with the panel closed, so the
// article banner reads the same state. Custom runtimes ride along: their
// own panel sections, their own export pop-up queue, and the export gate
// built on that queue.

import { useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import { desktopWidgetsIo } from '../lib/widgetApproval.tauri';
import { transport } from '../lib/event-transport';
import { eventOf } from '../lib/events';
import type { WidgetsStatus } from '../lib/generated/api';
import {
  createPromptQueue,
  createRuntimeModel,
  createRuntimePromptQueue,
  createWidgetsModel,
  offerRuntimeEvent,
  promptFromEvent,
  runtimeApprovalRef,
  runtimeEventFor,
  widgetEventFor,
  type PromptQueue,
  type RuntimeModel,
  type RuntimePromptQueue,
  type WidgetsModel,
} from '../lib/widgetApproval';

export interface RuntimeGate {
  status: (rootId: string, mainRel: string) => Promise<WidgetsStatus>;
  prompts: RuntimePromptQueue;
}

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
  const runtimeModel: RuntimeModel | null = useMemo(
    () => (target ? createRuntimeModel(desktopWidgetsIo, target.projectId, target.mainRel) : null),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [target?.projectId, target?.mainRel],
  );
  const runtimePrompts: RuntimePromptQueue | null = useMemo(
    () =>
      target ? createRuntimePromptQueue(desktopWidgetsIo, target.projectId, target.mainRel) : null,
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [target?.projectId, target?.mainRel],
  );
  const runtimeGate: RuntimeGate | null = useMemo(
    () =>
      runtimePrompts
        ? { status: (r, m) => desktopWidgetsIo.status(r, m), prompts: runtimePrompts }
        : null,
    [runtimePrompts],
  );

  useEffect(() => {
    if (!model || !prompts || !projectId) return;
    return transport().subscribe((e) => {
      const p = promptFromEvent(e, projectId);
      if (p) {
        prompts.enqueue(p);
        void model.refresh();
      }
      const acted = eventOf(e, 'widget.approved') ?? eventOf(e, 'widget.revoked');
      if (acted && acted.rootId === projectId) prompts.settled(acted.path);
      if (widgetEventFor(e, projectId)) void model.refresh();
      // A compile can add or drop held widgets without an approval event.
      if (e.event.action === 'compile.finish') void model.refresh();
    });
  }, [model, prompts, projectId]);

  useEffect(() => {
    if (!runtimeModel || !runtimePrompts || !projectId || !mainRel) return;
    return transport().subscribe((e) => {
      void offerRuntimeEvent(
        desktopWidgetsIo,
        (p) => runtimePrompts.enqueue(p),
        e,
        projectId,
        mainRel,
      );
      if (runtimeApprovalRef(e, projectId)) void runtimeModel.refresh();
      if (runtimeEventFor(e, projectId)) void runtimeModel.refresh();
      if (widgetEventFor(e, projectId)) void runtimeModel.refresh();
      // A compile can add or drop held runtimes without an approval event.
      if (e.event.action === 'compile.finish') void runtimeModel.refresh();
    });
  }, [runtimeModel, runtimePrompts, projectId, mainRel]);

  // The article banner reads the same listing the panel shows, so the
  // listing loads with the project and stays current without the panel.
  useEffect(() => {
    if (model) void model.refresh();
  }, [model]);

  useEffect(() => {
    if (runtimeModel) void runtimeModel.refresh();
  }, [runtimeModel]);

  useEffect(() => {
    if (panelOpen && model) void model.refresh();
  }, [panelOpen, model]);

  useEffect(() => {
    if (panelOpen && runtimeModel) void runtimeModel.refresh();
  }, [panelOpen, runtimeModel]);

  const state = useSyncExternalStore(
    (cb) => (model ? model.subscribe(cb) : () => {}),
    () => (model ? model.get() : null),
  );
  const promptState = useSyncExternalStore(
    (cb) => (prompts ? prompts.subscribe(cb) : () => {}),
    () => (prompts ? `${prompts.current()?.key ?? ''}|${prompts.failure() ?? ''}` : ''),
  );
  void promptState;
  const runtimeState = useSyncExternalStore(
    (cb) => (runtimeModel ? runtimeModel.subscribe(cb) : () => {}),
    () => (runtimeModel ? runtimeModel.get() : null),
  );
  const runtimePromptState = useSyncExternalStore(
    (cb) => (runtimePrompts ? runtimePrompts.subscribe(cb) : () => {}),
    () =>
      runtimePrompts
        ? `${runtimePrompts.current()?.key ?? ''}|${runtimePrompts.failure() ?? ''}`
        : '',
  );
  void runtimePromptState;

  return {
    panelOpen,
    openPanel: () => setPanelOpen(true),
    closePanel: () => setPanelOpen(false),
    model,
    state,
    prompts,
    /** Entry point for the poster renderer. */
    onApprovalRequired: prompts ? prompts.onApprovalRequired : null,
    runtimeModel,
    runtimeState,
    runtimePrompts,
    runtimePrompt: runtimePrompts ? runtimePrompts.current() : null,
    runtimeFailure: runtimePrompts ? runtimePrompts.failure() : null,
    /** The export gate: pending runtimes pop up before the export runs. */
    runtimeGate,
  };
}
