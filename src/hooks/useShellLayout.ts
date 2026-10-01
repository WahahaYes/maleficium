import { useCallback, useEffect, useState } from 'react';
import { DEVICE_PREF_KEYS, store } from '../lib/app-store';
import { presetOf } from '../lib/commands';

export interface PaneLayout {
  editorRatio: number;
  previewRatio: number;
  logHeight: number;
}

const DEFAULT_LAYOUT: PaneLayout = { editorRatio: 0.6, previewRatio: 0.4, logHeight: 160 };
const clampRatio = (r: number) => Math.min(0.8, Math.max(0.2, r));

export function parseLayout(raw: string | null | undefined): PaneLayout {
  if (!raw) return DEFAULT_LAYOUT;
  try {
    const j = JSON.parse(raw) as Partial<PaneLayout>;
    return {
      editorRatio: typeof j.editorRatio === 'number' ? clampRatio(j.editorRatio) : 0.6,
      previewRatio: typeof j.previewRatio === 'number' ? clampRatio(j.previewRatio) : 0.4,
      logHeight: typeof j.logHeight === 'number' ? Math.max(80, Math.min(600, j.logHeight)) : 160,
    };
  } catch {
    return DEFAULT_LAYOUT; // corrupted prefs: defaults win
  }
}

// Which panes are showing and how the space is split. The View menu presets
// own the visibility booleans; the split ratios persist per device.
export function useShellLayout() {
  const [treeVisible, setTreeVisible] = useState(true);
  const [editorVisible, setEditorVisible] = useState(true);
  const [previewOpen, setPreviewOpen] = useState(true);
  const [previewCollapsed, setPreviewCollapsed] = useState(false);
  const [logCollapsed, setLogCollapsed] = useState(false);
  const [outlineVisible, setOutlineVisible] = useState(true);
  const [layout, setLayout] = useState(() => parseLayout(store().get(DEVICE_PREF_KEYS.layout)));
  useEffect(() => {
    store().set(DEVICE_PREF_KEYS.layout, JSON.stringify(layout));
  }, [layout]);

  const setPreset = useCallback((preset: 'both' | 'editor' | 'preview') => {
    setTreeVisible(preset === 'both');
    setEditorVisible(preset !== 'preview');
    setPreviewOpen(preset !== 'editor');
    if (preset !== 'editor') setPreviewCollapsed(false);
  }, []);
  const showPreview = useCallback(() => {
    setPreviewOpen(true);
    setPreviewCollapsed(false);
  }, []);
  const togglePreview = useCallback(() => {
    setPreviewOpen((v) => !v);
    setPreviewCollapsed(false);
  }, []);
  const toggleTree = useCallback(() => setTreeVisible((v) => !v), []);
  const toggleLog = useCallback(() => setLogCollapsed((c) => !c), []);
  const toggleOutline = useCallback(() => setOutlineVisible((v) => !v), []);
  const collapsePreview = useCallback(() => setPreviewCollapsed(true), []);

  const setEditorRatio = useCallback(
    (r: number) => setLayout((l) => ({ ...l, editorRatio: r, previewRatio: 1 - r })),
    [],
  );
  const setPreviewRatio = useCallback(
    (r: number) => setLayout((l) => ({ ...l, previewRatio: r, editorRatio: 1 - r })),
    [],
  );
  const setLogHeight = useCallback((h: number) => setLayout((l) => ({ ...l, logHeight: h })), []);
  const dragSplitter = useCallback(
    (dx: number) =>
      setLayout((l) => {
        const r = clampRatio(l.editorRatio + dx / (window.innerWidth || 1000));
        return { ...l, editorRatio: r, previewRatio: 1 - r };
      }),
    [],
  );
  const nudgeSplitter = useCallback(
    (dir: number) =>
      setLayout((l) => {
        const r = clampRatio(l.editorRatio + dir * 0.05);
        return { ...l, editorRatio: r, previewRatio: 1 - r };
      }),
    [],
  );

  return {
    treeVisible,
    editorVisible,
    previewOpen,
    previewCollapsed,
    previewVisible: previewOpen && !previewCollapsed,
    logCollapsed,
    outlineVisible,
    layout,
    preset: presetOf({ tree: treeVisible, editor: editorVisible, preview: previewOpen }),
    view: { tree: treeVisible, editor: editorVisible, preview: previewOpen },
    setTreeVisible,
    setLogCollapsed,
    setPreset,
    showPreview,
    togglePreview,
    toggleTree,
    toggleLog,
    toggleOutline,
    collapsePreview,
    setEditorRatio,
    setPreviewRatio,
    setLogHeight,
    dragSplitter,
    nudgeSplitter,
  };
}
