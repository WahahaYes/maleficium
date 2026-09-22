// EditorViewport.tsx — CodeMirror 6 viewport editor.
//
// Viewport-render O(visible) only — never measures full content. Stable
// prop identity via memo + useCallback at call site.

import { memo, useEffect, useRef } from 'react';
import Paper from '@mui/material/Paper';
import { useTheme } from '@mui/material/styles';
import { EditorView, basicSetup } from 'codemirror';
import { Compartment, EditorState, EditorSelection } from '@codemirror/state';
import { texMode } from '../lib/texMode';
import { DEFAULT_PREFS } from '../lib/appearance';
import type { AppearancePrefs } from '../lib/appearance';
import { editorTheme } from '../lib/editorTheme';

export interface EditorViewportProps {
  value: string;
  onChange: (v: string) => void;
  onSave: () => void;
  line?: number;
  /** Bumped by inverse SyncTeX: flashes the revealed line amber 1.4s. */
  flashKey?: number;
}

/** Minimal viewport bridge: selection ops + caret line + click hook. */
export interface EditorViewportHandle {
  selectAll: () => void;
  expandSelection: () => void;
  shrinkSelection: () => void;
  goToLine: (line: number) => void;
  /** Current caret line (1-based) — drives double-click forward SyncTeX. */
  caretLine: () => number;
}

function EditorViewport({
  value,
  onChange,
  onSave,
  line,
  flashKey,
  viewportRef,
  onDoubleClickRef,
  filePath,
  prefs,
}: EditorViewportProps & {
  /** Drives select-all/expand/shrink/goto on the live view. */
  viewportRef?: React.MutableRefObject<EditorViewportHandle | null>;
  /** Double-click file + line for forward SyncTeX. */
  onDoubleClickRef?: React.MutableRefObject<((file: string, line: number) => void) | null>;
  /** Absolute path of the file in the viewport (captured at click time). */
  filePath?: string;
  /** Live appearance prefs driving editor font and size. */
  prefs?: AppearancePrefs;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const prefsRef = useRef(prefs ?? DEFAULT_PREFS);
  prefsRef.current = prefs ?? DEFAULT_PREFS;
  const filePathRef = useRef(filePath);
  filePathRef.current = filePath;
  // Last value sent downstream. Keystrokes update it synchronously in the
  // updateListener so an echo-back never triggers a full-doc replace.
  const lastSentRef = useRef(value);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onSaveRef = useRef(onSave);
  onSaveRef.current = onSave;
  const muiTheme = useTheme();
  // Theme compartment: prefs + tokens restyle the live view on mode flip.
  const themeCompartment = useRef(new Compartment()).current;

  useEffect(() => {
    if (!hostRef.current) return;
    const t0 = performance.now();
    const state = EditorState.create({
      doc: value,
      extensions: [
        basicSetup,
        texMode,
        // Wrap long lines instead of horizontal scroll.
        EditorView.lineWrapping,
        themeCompartment.of(editorTheme(prefsRef.current, muiTheme)),
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
          // Double-click = forward SyncTeX from the caret line; single click
          // stays caret-only. The file is captured at click time (state may
          // lag the visible buffer after a fast file switch + double-click).
          dblclick: (_e, view) => {
            try {
              const head = view.state.selection.main.head;
              onDoubleClickRef?.current?.(
                filePathRef.current ?? '',
                view.state.doc.lineAt(head).number,
              );
            } catch {
              /* no selection — ignore */
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
      console.timeLog?.(
        'editor',
        `editor render ${(value.length / 1_048_576).toFixed(1)}MB file in ${dt}ms`,
      );
    }
    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // Mount once; external value/line sync below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Restyle the live view when prefs or app theme change (mode flip).
  useEffect(() => {
    viewRef.current?.dispatch({
      effects: themeCompartment.reconfigure(editorTheme(prefsRef.current, muiTheme)),
    });
  }, [muiTheme, prefs, themeCompartment]);

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

  // Line reveal + amber flash on flashKey.
  const flashKeyRef = useRef(flashKey);
  useEffect(() => {
    const view = viewRef.current;
    if (!view || line == null || line < 1) return;
    try {
      const ln = view.state.doc.line(Math.min(line, view.state.doc.lines));
      view.dispatch({ selection: { anchor: ln.from }, scrollIntoView: true });
      view.focus();
    } catch {
      /* line out of range — ignore */
    }
    if (flashKeyRef.current !== flashKey) {
      flashKeyRef.current = flashKey;
      const dom = view.domAtPos(
        Math.min(line, view.state.doc.lines) >= 1
          ? (() => {
              try {
                return view.state.doc.line(Math.min(line, view.state.doc.lines)).from;
              } catch {
                return 0;
              }
            })()
          : 0,
      );
      const el = (
        dom?.node instanceof HTMLElement ? dom.node : dom?.node?.parentElement
      ) as HTMLElement | null;
      const lineEl = el?.closest?.('.cm-line') as HTMLElement | null;
      if (lineEl) {
        lineEl.classList.add('cm-synctex-flash');
        setTimeout(() => lineEl.classList.remove('cm-synctex-flash'), 1400);
      }
    }
  }, [line, flashKey]);

  // Viewport bridge drives the live view (no-ops unmounted); double-click
  // is exposed via onDoubleClickRef.
  useEffect(() => {
    if (!viewportRef) return;
    const stepOut = (dir: 1 | -1) => {
      const view = viewRef.current;
      if (!view) return;
      const sel = view.state.selection.main;
      const line = view.state.doc.lineAt(sel.head);
      const target = view.state.doc.line(
        Math.min(view.state.doc.lines, Math.max(1, line.number + dir)),
      );
      view.dispatch({
        selection: EditorSelection.range(sel.anchor, dir > 0 ? target.to : target.from),
        scrollIntoView: true,
      });
      view.focus();
    };
    viewportRef.current = {
      selectAll: () => {
        const view = viewRef.current;
        if (!view) return;
        view.dispatch({
          selection: { anchor: 0, head: view.state.doc.length },
          scrollIntoView: true,
        });
        view.focus();
      },
      expandSelection: () => stepOut(1),
      shrinkSelection: () => stepOut(-1),
      goToLine: (n: number) => {
        const view = viewRef.current;
        if (!view) return;
        try {
          const ln = view.state.doc.line(Math.min(Math.max(1, n), view.state.doc.lines));
          view.dispatch({ selection: { anchor: ln.from }, scrollIntoView: true });
          view.focus();
        } catch {
          /* out of range — ignore */
        }
      },
      caretLine: () => {
        const view = viewRef.current;
        if (!view) return 1;
        try {
          return view.state.doc.lineAt(view.state.selection.main.head).number;
        } catch {
          return 1;
        }
      },
    };
    return () => {
      viewportRef.current = null;
    };
  }, [viewportRef]);

  return (
    <Paper
      elevation={0}
      sx={{ p: 1, fontSize: 14, overflow: 'auto', border: 1, borderColor: 'divider' }}
    >
      <div ref={hostRef} />
    </Paper>
  );
}

export default memo(EditorViewport);
