// EditorViewport.tsx — CodeMirror 6 viewport editor.
//
// Renders only the visible lines, so cost does not grow with document
// length. Memoized: callers pass stable props (useCallback).

import { memo, useCallback, useEffect, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import Paper from '@mui/material/Paper';
import { useTheme } from '@mui/material/styles';
import { EditorView, basicSetup } from 'codemirror';
import { hoverTooltip, keymap } from '@codemirror/view';
import { Compartment, EditorState, EditorSelection, Prec } from '@codemirror/state';
import { search } from '@codemirror/search';
import { appOwnedEditorKeys } from '../lib/editorKeys';
import { texMode } from '../lib/texMode';
import { DEFAULT_PREFS } from '../lib/appearance';
import type { AppearancePrefs } from '../lib/appearance';
import { editorTheme } from '../lib/editorTheme';
import FindBar from './FindBar';

export interface EditorViewportProps {
  value: string;
  onChange: (v: string) => void;
  onSave: () => void;
  line?: number;
  /** Bumped by inverse SyncTeX: flashes the revealed line amber 1.4s. */
  flashKey?: number;
  /** Select a range (1-based line, UTF-16 column and length) once per key. */
  select?: { line: number; col: number; len: number; key: number };
}

/** Minimal viewport bridge: selection ops + caret line + click hook. */
export interface EditorViewportHandle {
  selectAll: () => void;
  expandSelection: () => void;
  shrinkSelection: () => void;
  goToLine: (line: number) => void;
  /** Open the in-file find panel. */
  openFind: () => void;
  /** The caret's line text and UTF-16 column in it. */
  caretAt: () => { text: string; col: number } | null;
  /** Current caret line (1-based) — drives double-click forward SyncTeX. */
  caretLine: () => number;
}

function EditorViewport({
  value,
  onChange,
  onSave,
  line,
  flashKey,
  select,
  viewportRef,
  onDoubleClickRef,
  filePath,
  prefs,
  definitionRef,
}: EditorViewportProps & {
  /** Drives select-all/expand/shrink/goto on the live view. */
  viewportRef?: React.MutableRefObject<EditorViewportHandle | null>;
  /** Double-click file + line for forward SyncTeX. */
  onDoubleClickRef?: React.MutableRefObject<((file: string, line: number) => void) | null>;
  /** Absolute path of the file in the viewport (captured at click time). */
  filePath?: string;
  /** Live appearance prefs driving editor font and size. */
  prefs?: AppearancePrefs;
  /** Definitions: hover text for a line + column, and Ctrl+click to go there. */
  definitionRef?: React.MutableRefObject<{
    hover: (line: string, col: number) => Promise<string | null>;
    go: (line: string, col: number) => void;
  } | null>;
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
  // MUI find bar state: opened by Ctrl+F or the menu, seeded from selection.
  const [findOpen, setFindOpen] = useState(false);
  const [findSeed, setFindSeed] = useState('');
  const [findKey, setFindKey] = useState(0);
  const viewOf = useCallback(() => viewRef.current, []);
  const doOpenFind = useCallback(() => {
    const view = viewRef.current;
    let seed = '';
    if (view) {
      const sel = view.state.selection.main;
      if (!sel.empty && sel.to - sel.from <= 100) {
        seed = view.state.sliceDoc(sel.from, sel.to);
      }
    }
    setFindSeed(seed);
    setFindKey((k) => k + 1);
    setFindOpen(true);
  }, []);
  const openBarRef = useRef(() => {});
  openBarRef.current = doOpenFind;

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
        // In-file find uses the MUI bar below, never the stock panel.
        search(),
        Prec.highest(keymap.of(appOwnedEditorKeys(() => openBarRef.current()))),
        // Hover a reference, macro or input: where it is defined.
        hoverTooltip(async (view, pos) => {
          const hover = definitionRef?.current?.hover;
          if (!hover) return null;
          const line = view.state.doc.lineAt(pos);
          const text = await hover(line.text, pos - line.from);
          if (!text) return null;
          return {
            pos,
            above: true,
            create: () => {
              const dom = document.createElement('div');
              dom.className = 'cm-definition-hover';
              dom.style.whiteSpace = 'pre-wrap';
              dom.style.maxWidth = '60ch';
              dom.style.padding = '2px 6px';
              dom.textContent = text;
              return { dom };
            },
          };
        }),
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
          // Ctrl+click = go to definition of what was clicked.
          mousedown: (e, view) => {
            if (!(e.ctrlKey || e.metaKey) || !definitionRef?.current) return false;
            const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
            if (pos == null) return false;
            const line = view.state.doc.lineAt(pos);
            e.preventDefault();
            definitionRef.current.go(line.text, pos - line.from);
            return true;
          },
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

  // Range reveal (a search hit): applied once per key, after the value sync
  // above has loaded the file it belongs to.
  const selectKeyRef = useRef<number | null>(null);
  useEffect(() => {
    const view = viewRef.current;
    if (!view || !select || selectKeyRef.current === select.key) return;
    selectKeyRef.current = select.key;
    try {
      const ln = view.state.doc.line(Math.min(Math.max(1, select.line), view.state.doc.lines));
      const from = Math.min(ln.from + select.col, ln.to);
      const to = Math.min(from + select.len, view.state.doc.length);
      view.dispatch({ selection: { anchor: from, head: to }, scrollIntoView: true });
      view.focus();
    } catch {
      /* the document changed under the hit — ignore */
    }
  }, [select, value]);

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
                // The document shrank under the request: reveal the top.
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
      caretAt: () => {
        const view = viewRef.current;
        if (!view) return null;
        const head = view.state.selection.main.head;
        const line = view.state.doc.lineAt(head);
        return { text: line.text, col: head - line.from };
      },
      openFind: () => {
        doOpenFind();
      },
      caretLine: () => {
        const view = viewRef.current;
        if (!view) return 1;
        try {
          return view.state.doc.lineAt(view.state.selection.main.head).number;
        } catch {
          // Selection outside a document mid-replace: report line 1.
          return 1;
        }
      },
    };
    return () => {
      viewportRef.current = null;
    };
  }, [viewportRef, doOpenFind]);

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
      <Paper
        elevation={0}
        sx={{
          p: 1,
          fontSize: 14,
          overflow: 'auto',
          border: 1,
          borderColor: 'divider',
          flex: 1,
          minHeight: 0,
        }}
      >
        <div ref={hostRef} />
      </Paper>
      {findOpen ? (
        <FindBar
          key={findKey}
          viewOf={viewOf}
          seed={findSeed}
          fontSizePx={Math.max(10, (prefs ?? DEFAULT_PREFS).editorSize - 2)}
          onClose={() => setFindOpen(false)}
        />
      ) : null}
    </Box>
  );
}

export default memo(EditorViewport);
