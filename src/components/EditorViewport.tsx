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
  /** Bumped by inverse SyncTeX: flashes the revealed line amber 1.4s. */
  flashKey?: number;
  hideChrome?: boolean;
  /** Stub: logs + window.open fallback; Tauri Window plugin is a future session. */
  popout?: boolean;
  collapsed?: boolean;
}

function EditorViewport({ value, onChange, onSave, line, flashKey, hideChrome, popout, collapsed }: EditorViewportProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  // Last value WE sent downstream (mount doc or external sync). Keystrokes
  // update this synchronously in the updateListener so the [value] echo-back
  // from App state never triggers a full-doc replace (cursor jump).
  const lastSentRef = useRef(value);
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
          if (u.docChanged) {
            lastSentRef.current = u.state.doc.toString();
            onChangeRef.current(lastSentRef.current);
          }
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
  // Guarded by ref-equality with last-sent value so typing never resets cursor.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    if (lastSentRef.current === value) return;
    lastSentRef.current = value;
    const cur = view.state.doc.toString();
    if (cur !== value) {
      view.dispatch({ changes: { from: 0, to: cur.length, insert: value } });
    }
  }, [value]);

  // Line reveal (Problems jump / inverse SyncTeX) + amber flash on flashKey.
  const flashKeyRef = useRef(flashKey);
  useEffect(() => {
    const view = viewRef.current;
    if (!view || line == null || line < 1) return;
    try {
      const ln = view.state.doc.line(Math.min(line, view.state.doc.lines));
      view.dispatch({ selection: { anchor: ln.from }, scrollIntoView: true });
      view.focus();
    } catch { /* line out of range — ignore */ }
    if (flashKeyRef.current !== flashKey) {
      flashKeyRef.current = flashKey;
      const dom = view.domAtPos(Math.min(line, view.state.doc.lines) >= 1
        ? (() => { try { return view.state.doc.line(Math.min(line, view.state.doc.lines)).from; } catch { return 0; } })()
        : 0);
      const el = (dom?.node instanceof HTMLElement ? dom.node : dom?.node?.parentElement) as HTMLElement | null;
      const lineEl = el?.closest?.('.cm-line') as HTMLElement | null;
      if (lineEl) {
        lineEl.classList.add('cm-synctex-flash');
        setTimeout(() => lineEl.classList.remove('cm-synctex-flash'), 1400);
      }
    }
  }, [line, flashKey]);

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
