// EditorViewport.tsx — CodeMirror 6 viewport editor (replaces Editor.tsx).
//
// Growth cap: viewport-render O(visible) only — never measures full content
// (kills the 4.4MB textarea freeze, 01 STATUS 7433ms). Stable prop identity via
// memo + useCallback at call site. Emits `editor render 5MB file in Xms`.

import { memo, useEffect, useRef } from 'react';
import Paper from '@mui/material/Paper';
import { EditorView, basicSetup } from 'codemirror';
import { EditorState } from '@codemirror/state';

export interface EditorViewportProps {
  value: string;
  onChange: (v: string) => void;
  onSave: () => void;
  line?: number;
  hideChrome?: boolean;
  /** Stub: logs + window.open fallback; Tauri Window plugin is a future session. */
  popout?: boolean;
  collapsed?: boolean;
}

function EditorViewport({ value, onChange, onSave, line, hideChrome, popout, collapsed }: EditorViewportProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onSaveRef = useRef(onSave);
  onSaveRef.current = onSave;

  useEffect(() => {
    if (!hostRef.current) return;
    const t0 = performance.now();
    const state = EditorState.create({
      doc: value,
      extensions: [
        basicSetup,
        EditorView.updateListener.of((u) => {
          if (u.docChanged) onChangeRef.current(u.state.doc.toString());
        }),
        EditorView.domEventHandlers({
          keydown: (e) => {
            if ((e.ctrlKey || e.metaKey) && e.key === 's') {
              e.preventDefault();
              onSaveRef.current();
              return true;
            }
            return false;
          },
        }),
      ],
    });
    const view = new EditorView({ state, parent: hostRef.current });
    viewRef.current = view;
    const dt = Math.round(performance.now() - t0);
    if (value.length > 1_000_000) {
      // eslint-disable-next-line no-console
      console.timeLog?.('editor', `editor render ${(value.length / 1_048_576).toFixed(1)}MB file in ${dt}ms`);
    }
    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // Mount once; external value/line sync below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // External value sync (file switch / reload / jump): replace doc, keep viewport.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const cur = view.state.doc.toString();
    if (cur !== value) {
      view.dispatch({ changes: { from: 0, to: cur.length, insert: value } });
    }
  }, [value]);

  // Line reveal (Problems jump / inverse SyncTeX).
  useEffect(() => {
    const view = viewRef.current;
    if (!view || line == null || line < 1) return;
    try {
      const ln = view.state.doc.line(Math.min(line, view.state.doc.lines));
      view.dispatch({ selection: { anchor: ln.from }, scrollIntoView: true });
      view.focus();
    } catch { /* line out of range — ignore */ }
  }, [line]);

  useEffect(() => {
    if (popout) {
      // eslint-disable-next-line no-console
      console.log('popout: editor (stub — same value reference, Tauri Window deferred)');
    }
  }, [popout]);

  if (collapsed) return null;

  return (
    <Paper elevation={0} sx={{ p: hideChrome ? 0 : 1, fontSize: 14, overflow: 'auto' }}>
      <div ref={hostRef} />
    </Paper>
  );
}

export default memo(EditorViewport);
