// useSynctex.ts — forward and inverse SyncTeX between editor and preview.
//
// Owns the caret line, the preview page, and the flash counter. Both
// directions are disabled while a compile runs: the .synctex.gz is being
// rewritten, so any answer would describe the previous document.

import { useRef, useState } from 'react';
import type { RefObject } from 'react';
import type { EditorViewportHandle } from '../components/EditorViewport';
import type { CompilePhase } from './useCompileRunner';
import { enforceBufferCap, getOrCreateBuffer, type BufferState } from '../lib/buffers';
import { emit } from '../lib/events';
import { loadTex } from '../lib/files';
import {
  forward_sync,
  inverse_sync,
  isCrossFileHit,
  isForwardNoMatch,
  parseForwardSync,
  parseInverseSync,
  shouldTurnPage,
  syncAvailable,
  texPathFor,
} from '../lib/synctex';

export interface UseSynctexDeps {
  pdfUrl: string | null;
  compilePhase: CompilePhase;
  fileName: string;
  workdirHint: string;
  viewportRef: RefObject<EditorViewportHandle | null>;
  fileNameRef: RefObject<string>;
  setBuffers: React.Dispatch<React.SetStateAction<Map<string, BufferState>>>;
  setTex: (v: string) => void;
  setFileName: (v: string) => void;
  setPreviewFile: (v: string | null) => void;
  setLargeFile: (v: string | null) => void;
  setReloadPath: (v: string | null) => void;
  /** Latest-closure handle for the keymap listener and the menu dispatcher. */
  forwardSyncRef: RefObject<() => Promise<void>>;
  /** Latest-closure handle for the editor's double-click bridge. */
  forwardSyncLineRef: RefObject<(file: string, line: number) => void>;
}

export function useSynctex(deps: UseSynctexDeps) {
  const {
    pdfUrl,
    compilePhase,
    fileName,
    workdirHint,
    viewportRef,
    fileNameRef,
    setBuffers,
    setTex,
    setFileName,
    setPreviewFile,
    setLargeFile,
    setReloadPath,
    forwardSyncRef,
    forwardSyncLineRef,
  } = deps;

  const [currentLine, setCurrentLine] = useState(1);
  const currentLineRef = useRef(currentLine);
  currentLineRef.current = currentLine;
  const [synctexFlash, setSynctexFlash] = useState(0);
  const [pageNumber, setPageNumber] = useState(1);
  const pageNumberRef = useRef(pageNumber);
  pageNumberRef.current = pageNumber;

  async function handleForwardSync() {
    if (!pdfUrl) return;
    if (!syncAvailable(pdfUrl, compilePhase === 'compiling')) {
      emit({
        scope: 'preview',
        kind: 'warn',
        message: 'SyncTeX unavailable while compiling (synctex_no_match)',
      });
      return;
    }
    // The caret may have moved since the last jump: read the live line from
    // the viewport bridge (currentLine only tracks jumps, not caret moves).
    const liveLine = viewportRef.current?.caretLine() ?? currentLineRef.current;
    if (liveLine !== currentLineRef.current) setCurrentLine(liveLine);
    const result = await forward_sync(pdfUrl, texPathFor(fileName, workdirHint), liveLine);
    if (!result.ok) {
      emit({
        scope: 'preview',
        kind: 'warn',
        message: `SyncTeX query failed (${result.text.slice(0, 200)})`,
      });
      return;
    }
    if (isForwardNoMatch(result.text)) {
      emit({ scope: 'preview', kind: 'warn', message: 'synctex_no_match' });
      return;
    }
    const target = parseForwardSync(result.text);
    if (target == null) {
      emit({
        scope: 'preview',
        kind: 'info',
        message: `forward SyncTeX → ${result.text.slice(0, 120)}`,
      });
      return;
    }
    // Preamble/untagged lines resolve to a same-page rect with no movement:
    // arriving without moving is noise, not navigation — stay silent.
    if (!shouldTurnPage(target, pageNumberRef.current)) return;
    setPageNumber(target);
    emit({ scope: 'preview', kind: 'info', message: `forward SyncTeX → page ${target}` });
  }

  async function handleInverseSync(page: number, x: number, y: number) {
    if (!pdfUrl) return;
    if (!syncAvailable(pdfUrl, compilePhase === 'compiling')) {
      emit({
        scope: 'preview',
        kind: 'warn',
        message: 'synctex_no_match: disabled during compile',
      });
      return;
    }
    const result = await inverse_sync(pdfUrl, page, x, y);
    if (!result.ok) {
      emit({
        scope: 'preview',
        kind: 'warn',
        message: `SyncTeX query failed (${result.text.slice(0, 200)})`,
      });
      return;
    }
    // Real `synctex edit` shape:
    //   Input:/abs/path/hello.tex\nLine:7\n...
    const { line, hitFile } = parseInverseSync(result.text);
    if (line != null) {
      // Jump the owning file when SyncTeX names one (multi-file projects);
      // otherwise reveal the line in the current buffer.
      if (isCrossFileHit(hitFile, fileName)) {
        try {
          const content = await loadTex(hitFile);
          setBuffers((b) => {
            const n = new Map(b);
            getOrCreateBuffer(n, hitFile, content);
            return enforceBufferCap(n, fileNameRef.current);
          });
          setTex(content);
          setFileName(hitFile);
          setPreviewFile(null);
          setLargeFile(null);
          setReloadPath(null);
        } catch {
          /* unreadable hit file — still reveal the line number below */
        }
      }
      setCurrentLine(line);
      setSynctexFlash((f) => f + 1);
      emit({
        scope: 'preview',
        kind: 'success',
        message: `synctex inverse → ${hitFile ?? fileName}:${line}`,
      });
    } else {
      emit({
        scope: 'preview',
        kind: 'warn',
        message: 'SyncTeX: no match at this position (synctex_no_match)',
      });
    }
  }

  forwardSyncRef.current = handleForwardSync;
  forwardSyncLineRef.current = (file: string, line: number) => {
    if (!pdfUrl || !syncAvailable(pdfUrl, compilePhase === 'compiling')) return;
    if (line !== currentLineRef.current) setCurrentLine(line);
    void forward_sync(pdfUrl, texPathFor(file, workdirHint), line).then((result) => {
      if (isForwardNoMatch(result.text)) {
        emit({ scope: 'preview', kind: 'warn', message: 'synctex_no_match' });
        return;
      }
      if (!result.ok) {
        emit({
          scope: 'preview',
          kind: 'warn',
          message: `SyncTeX query failed (${result.text.slice(0, 200)})`,
        });
        return;
      }
      const target = parseForwardSync(result.text);
      if (!shouldTurnPage(target, pageNumberRef.current)) return;
      setPageNumber(target);
      emit({ scope: 'preview', kind: 'info', message: `forward SyncTeX → page ${target}` });
    });
  };

  return {
    currentLine,
    setCurrentLine,
    synctexFlash,
    pageNumber,
    setPageNumber,
    handleForwardSync,
    handleInverseSync,
  };
}
