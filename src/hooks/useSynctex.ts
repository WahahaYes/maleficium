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
import type { PreviewSource } from '../lib/preview-bus';
import {
  forward_sync,
  inverse_sync,
  isCrossFileHit,
  relTo,
  shouldTurnPage,
  syncAvailable,
  texPathFor,
} from '../lib/synctex';
import { joinPath } from '../lib/paths';

export interface UseSynctexDeps {
  pdfUrl: string | null;
  /** The main file behind the shown pdf; null when it has no session root. */
  source: PreviewSource | null;
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
  /** Latest-closure handle for the keymap listener and the menu dispatcher. */
  forwardSyncRef: RefObject<() => Promise<void>>;
  /** Latest-closure handle for the editor's double-click bridge. */
  forwardSyncLineRef: RefObject<(file: string, line: number) => void>;
}

export function useSynctex(deps: UseSynctexDeps) {
  const {
    pdfUrl,
    source,
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

  /** Forward query for `file`: the target page, or null after reporting why. */
  async function forwardPage(file: string, line: number): Promise<number | null> {
    const texRel = source ? relTo(source.rootPath, texPathFor(file, workdirHint)) : null;
    if (!source || !texRel) {
      emit({
        scope: 'preview',
        kind: 'warn',
        actor: 'user',
        message: 'SyncTeX: source is outside the project',
        event: { action: 'synctex.outside', direction: 'forward' },
      });
      return null;
    }
    const result = await forward_sync(source.rootId, source.mainRel, texRel, line);
    if (!result.ok) {
      emit({
        scope: 'preview',
        kind: 'warn',
        actor: 'user',
        message: `SyncTeX query failed (${(result.error ?? '').slice(0, 200)})`,
        event: {
          action: 'synctex.failed',
          direction: 'forward',
          error: (result.error ?? '').slice(0, 200),
        },
      });
      return null;
    }
    if (result.page == null) {
      emit({
        scope: 'preview',
        kind: 'warn',
        actor: 'user',
        message: 'synctex_no_match',
        event: { action: 'synctex.no-match', direction: 'forward', compiling: false },
      });
    }
    return result.page;
  }

  async function handleForwardSync() {
    if (!pdfUrl) return;
    if (!syncAvailable(pdfUrl, compilePhase === 'compiling')) {
      emit({
        scope: 'preview',
        kind: 'warn',
        actor: 'user',
        message: 'SyncTeX unavailable while compiling (synctex_no_match)',
        event: { action: 'synctex.no-match', direction: 'forward', compiling: true },
      });
      return;
    }
    // The caret may have moved since the last jump: read the live line from
    // the viewport bridge (currentLine only tracks jumps, not caret moves).
    const liveLine = viewportRef.current?.caretLine() ?? currentLineRef.current;
    if (liveLine !== currentLineRef.current) setCurrentLine(liveLine);
    const target = await forwardPage(fileName, liveLine);
    // Preamble/untagged lines resolve to a same-page rect with no movement:
    // arriving without moving is noise, not navigation — stay silent.
    if (!shouldTurnPage(target, pageNumberRef.current)) return;
    setPageNumber(target);
    emit({
      scope: 'preview',
      kind: 'info',
      actor: 'user',
      message: `forward SyncTeX → page ${target}`,
      event: { action: 'synctex.forward', page: target },
    });
  }

  async function handleInverseSync(page: number, x: number, y: number) {
    if (!pdfUrl) return;
    if (!syncAvailable(pdfUrl, compilePhase === 'compiling')) {
      emit({
        scope: 'preview',
        kind: 'warn',
        actor: 'user',
        message: 'synctex_no_match: disabled during compile',
        event: { action: 'synctex.no-match', direction: 'inverse', compiling: true },
      });
      return;
    }
    if (!source) {
      emit({
        scope: 'preview',
        kind: 'warn',
        actor: 'user',
        message: 'SyncTeX: output is outside the project',
        event: { action: 'synctex.outside', direction: 'inverse' },
      });
      return;
    }
    const result = await inverse_sync(source.rootId, source.mainRel, page, x, y);
    if (!result.ok) {
      emit({
        scope: 'preview',
        kind: 'warn',
        actor: 'user',
        message: `SyncTeX query failed (${(result.error ?? '').slice(0, 200)})`,
        event: {
          action: 'synctex.failed',
          direction: 'inverse',
          error: (result.error ?? '').slice(0, 200),
        },
      });
      return;
    }
    const { line } = result;
    const hitFile = result.relPath ? joinPath(source.rootPath, result.relPath) : null;
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
        } catch {
          /* unreadable hit file — still reveal the line number below */
        }
      }
      setCurrentLine(line);
      setSynctexFlash((f) => f + 1);
      emit({
        scope: 'preview',
        kind: 'success',
        actor: 'user',
        message: `synctex inverse → ${hitFile ?? fileName}:${line}`,
        event: { action: 'synctex.inverse', path: hitFile ?? fileName, line },
      });
    } else {
      emit({
        scope: 'preview',
        kind: 'warn',
        actor: 'user',
        message: 'SyncTeX: no match at this position (synctex_no_match)',
        event: { action: 'synctex.no-match', direction: 'inverse', compiling: false },
      });
    }
  }

  forwardSyncRef.current = handleForwardSync;
  forwardSyncLineRef.current = (file: string, line: number) => {
    if (!pdfUrl || !syncAvailable(pdfUrl, compilePhase === 'compiling')) return;
    if (line !== currentLineRef.current) setCurrentLine(line);
    void forwardPage(file, line).then((target) => {
      if (!shouldTurnPage(target, pageNumberRef.current)) return;
      setPageNumber(target);
      emit({
        scope: 'preview',
        kind: 'info',
        actor: 'user',
        message: `forward SyncTeX → page ${target}`,
        event: { action: 'synctex.forward', page: target },
      });
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
