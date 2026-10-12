import { useCallback, useEffect, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import CompileButton from './components/CompileButton';
import MenuBar from './components/MenuBar';
import LogStream, { type ProblemRef } from './components/LogStream';
import { baseName, dirName, hasDir, joinPath, relUnder } from './lib/paths';
import { useDevCamera } from './hooks/useDevCamera';
import { type EditorViewportHandle } from './components/EditorViewport';
import EditorPane from './components/EditorPane';
import PreviewPane from './components/PreviewPane';
import SideColumn from './components/SideColumn';
import WorkArea from './components/WorkArea';
import { mainFileTip } from './lib/mainFileTip';
import { paletteCommands } from './lib/palette';
import { publishPresence } from './lib/presence';
import type { AppearancePrefs } from './lib/appearance';
import StatusBar from './components/StatusBar';
import { useExternalChanges } from './hooks/useExternalChanges';
import { listDir1Level, loadTex, TreeEntry } from './lib/files';
import { getOrCreateBuffer, updateBuffer, enforceBufferCap } from './lib/buffers';
import { compileLogTitle } from './lib/compile';
import { onPdf, type PreviewDoc } from './lib/preview-bus';
import { emit } from './lib/events';
import { startEventLog } from './lib/eventlog';
import { historyAvailability } from './lib/history.view';
import type { ZoomAction } from './lib/zoom';
import { useExport } from './hooks/useExport';
import { useWidgetApproval } from './hooks/useWidgetApproval';
import { buildMenus, type MenuContext } from './lib/commands';
import { commandActions } from './lib/commandActions';
import { FileHistory } from './lib/file-history';
import { grantUntitledAccess } from './lib/projectAccess';
import { useProjectReplace } from './hooks/useProjectReplace';
import { useFileSelection } from './hooks/useFileSelection';
import { useMainFile } from './hooks/useMainFile';
import { useSaveFile } from './hooks/useSaveFile';
import { useGlobalKeymap } from './hooks/useGlobalKeymap';
import { useShellLayout } from './hooks/useShellLayout';
import { useBufferManager } from './hooks/useBufferManager';
import { useCompileRunner } from './hooks/useCompileRunner';
import AppDialogs from './components/AppDialogs';
import { useArticleSync } from './hooks/useArticleSync';
import { useDialogs } from './hooks/useDialogs';
import { useNavigation } from './hooks/useNavigation';
import { useOutline } from './hooks/useOutline';
import { useProjectRoot } from './hooks/useProjectRoot';
import { useProjectTree } from './hooks/useProjectTree';
import { useFileOps } from './hooks/useFileOps';
import { useRevisionHistory } from './hooks/useRevisionHistory';
import { useIndexOverlays } from './hooks/useIndexOverlays';
import { useSynctex } from './hooks/useSynctex';

const HELLO = '\\documentclass{article}\n\\begin{document}\nHello Maleficium\n\\end{document}\n';

export default function App({
  themeMode = 'dark',
  onThemeMode = () => {},
  density = 'comfortable',
  onDensityMode = () => {},
  prefs,
  onPrefs = () => {},
}: {
  themeMode?: 'dark' | 'light';
  onThemeMode?: (m: 'dark' | 'light') => void;
  density?: 'comfortable' | 'compact';
  onDensityMode?: (d: 'comfortable' | 'compact') => void;
  prefs: AppearancePrefs;
  onPrefs?: (p: AppearancePrefs) => void;
}) {
  const shell = useShellLayout();
  const { toggleTree } = shell;
  const [tex, setTex] = useState(HELLO);
  const { root, projectId, rootRef, projectIdRef, setRootNow, setProjectIdNow, relInProject } =
    useProjectRoot();
  const [tree, setTree] = useState<TreeEntry[]>([]);
  const [fileName, setFileName] = useState('hello.tex');
  const [trash] = useState(() => new FileHistory());

  const [log, setLog] = useState('ready');
  const [largeFile, setLargeFile] = useState<string | null>(null);
  // Non-text selection (image/video/pdf/binary): rich preview, never the editor.
  const [previewFile, setPreviewFile] = useState<string | null>(null);
  const [previewDoc, setPreviewDoc] = useState<PreviewDoc | null>(null);
  const pdfUrl = previewDoc?.url ?? null;
  // Ref mirror for the watcher closure (the effect is root-scoped).
  const fileNameRef = useRef(fileName);
  fileNameRef.current = fileName;
  // Latest closures for the subscribe-once global keymap listener. The
  // listener never closes over render state: assign every render and call
  // only `*.current()`. Untitled typing updates `tex` alone, so a dep-driven
  // listener would never resubscribe.
  const compileRef = useRef<() => Promise<void>>(async () => {});
  const forwardSyncRef = useRef<() => Promise<void>>(async () => {});
  const forwardSyncLineRef = useRef<(file: string, line: number) => void>(() => {});
  const handleSelectRef = useRef<(path: string) => Promise<void>>(async () => {});
  // Viewport bridge assigned via viewportRef prop. Without it
  // selectAll/expand/shrink/goToLine no-op.
  const viewportRef = useRef<EditorViewportHandle | null>(null);
  const { buffers, setBuffers, buffersRef, handleCloseBuffer, handleCloseOthers, handleCloseAll } =
    useBufferManager({
      fileName,
      setFileName,
      previewFile,
      setPreviewFile,
      setTex,
      emptyTex: HELLO,
      setLargeFile,
    });
  const { conflicts, checkExternal, resolveExternal } = useExternalChanges({
    buffers,
    setBuffers,
    fileName,
    setTex,
  });

  // Revision history: app-local, keyed by the backend-minted project id.
  const history = useRevisionHistory({
    root,
    projectId,
    relInProject,
    fileName,
    setBuffers,
    setTex,
    setLog,
  });

  useIndexOverlays(projectId, buffers, relInProject);

  // The bus is recorded to an app-local JSONL file for the length of the run.
  useEffect(() => {
    const log = startEventLog();
    return () => log.stop();
  }, []);

  useEffect(() => onPdf(setPreviewDoc), []);

  const {
    mainFile,
    mainSource,
    mainCandidates,
    setMainFileState,
    clearMain,
    resolveMain,
    setMainToOpenFile,
    setMainToPath,
    pickMain,
  } = useMainFile({ root, projectId, rootRef, setLog });
  // What this window has open, for the MCP server's read view of the app.
  useEffect(() => {
    publishPresence(
      projectId,
      mainFile ? relInProject(mainFile) : null,
      projectId ? relInProject(fileName) : null,
    );
  }, [projectId, root, mainFile, fileName, relInProject]);
  const handleSelect = useFileSelection({
    fileName,
    fileNameRef,
    buffers,
    setBuffers,
    setTex,
    setFileName,
    setPreviewFile,
    setLargeFile,
    setLog,
    noteSavedRevision: history.noteSavedRevision,
    resolveMain,
    rootRef,
    projectIdRef,
  });
  // Callers defined before handleSelect (tab cycling, go to definition,
  // search hits) switch files through this ref.
  handleSelectRef.current = handleSelect;
  // Tree-driven main association (double-click / context menu on a .tex row).
  const selectAndSetMain = async (path: string) => {
    await handleSelect(path);
    await setMainToPath(path);
  };

  const menuActionRef = useRef<(id: string) => void>(() => {});
  const zoomActionRef = useRef<((a: ZoomAction) => void) | null>(null);
  useGlobalKeymap(
    {
      compile: () => void compileRef.current(),
      goToDefinition: () => nav.goToDefinitionRef.current(),
      forwardSync: () => void forwardSyncRef.current(),
      zoom: (z) => zoomActionRef.current?.(z),
      menuAction: (id) => menuActionRef.current(id),
      showShortcuts: () => dialogs.show('shortcuts'),
      toggleTree,
      select: (path) => void handleSelectRef.current(path),
    },
    buffersRef,
    fileNameRef,
  );

  // Untitled documents resolve against the backend-owned scratch root.
  const [scratch, setScratch] = useState<{ rootId: string; path: string } | null>(null);
  useEffect(() => {
    void grantUntitledAccess().then((g) => {
      if (g.path && g.rootId) setScratch({ rootId: g.rootId, path: g.path });
    });
  }, []);
  const workdirHint = hasDir(fileName) ? dirName(fileName) : (scratch?.path ?? '');
  const mainDir = mainFile ? dirName(mainFile) : workdirHint;
  // Repo-relative for display (absolute kept in tooltips); plain language.
  const relOf = (abs: string | null): string | null => {
    if (!abs) return null;
    return (root ? relUnder(root, abs) : null) ?? abs;
  };

  const save = useSaveFile({
    fileName,
    tex,
    buffers,
    setBuffers,
    largeFile,
    noteSavedRevision: history.noteSavedRevision,
    setLog,
  });

  /** One tree level on expand; an unreadable folder says so and lists empty. */
  async function expandDir(dir: string): Promise<TreeEntry[]> {
    try {
      return await listDir1Level(dir);
    } catch (e) {
      emit({
        scope: 'fs',
        kind: 'error',
        actor: 'user',
        message: 'could not list ' + dir,
        event: { action: 'tree.load-failed', dir, error: String(e).slice(0, 200) },
      });
      return [];
    }
  }

  /** Jump to a root-relative problem location inside a granted root. */
  function handleProblemJump(t: ProblemRef) {
    const base = t.rootId === projectId ? root : t.rootId === scratch?.rootId ? scratch.path : null;
    if (base) handleJump(joinPath(base, t.path), t.line);
  }

  function handleJump(absPath: string, line: number) {
    loadTex(absPath).then((content) => {
      setBuffers((b) => {
        const n = new Map(b);
        getOrCreateBuffer(n, absPath, content);
        return enforceBufferCap(n, fileNameRef.current);
      });
      setTex(content);
      setFileName(absPath);
      setPreviewFile(null);
      setLargeFile(null);
      setCurrentLine(line);
    });
  }

  const { logCollapsed, setLogCollapsed, layout } = shell;
  const {
    compilePhase,
    compileTimer,
    offline,
    compiled,
    progress,
    autoCompile,
    setAutoCompile,
    makeOffline,
    warmCompile,
    handleCompileFile,
    precheck,
    precheckOpen,
    openPrecheck,
    closePrecheck,
    precheckPopup,
    setPrecheckPopup,
  } = useCompileRunner({
    tex,
    fileName,
    mainFile,
    setMainFileState,
    mainDir,
    workdirHint,
    root,
    projectId,
    scratch,
    buffers,
    setBuffers,
    largeFile,
    previewFile,
    setLog,
    setLogCollapsed,
    compileRef,
  });
  const { reloadTree, openRoot, open, openWelcome, recentProjects, setRecentProjects } =
    useProjectTree({
      root,
      projectId,
      setRoot: setRootNow,
      setProjectId: setProjectIdNow,
      setTree,
      fileNameRef,
      checkExternal,
      setLog,
      trash,
      resolveMain,
      clearMainFile: clearMain,
      handleSelect,
      warmCompile,
      closeAll: handleCloseAll,
    });
  // An agent's ask the user accepted: the File > Open path (or the open
  // project already), then the named file.
  const openForAgent = useCallback(
    async (project: string, file: string | null) => {
      if (rootRef.current !== project) await openRoot(project);
      if (file) await handleSelectRef.current(joinPath(project, file));
    },
    [openRoot, rootRef],
  );
  const { handleCreate, handleRename, handleDelete, handleClean, handleUndo } = useFileOps({
    root,
    projectId,
    scratch,
    fileName,
    previewFile,
    setFileName,
    mainFile,
    setPreviewFile,
    setBuffers,
    trash,
    reloadTree,
    handleSelect,
  });
  const dialogs = useDialogs();
  // Project search replaces the tree + outline in the side column while open.
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchFocus, setSearchFocus] = useState(0);
  const nav = useNavigation({
    root,
    projectId,
    mainRel: mainFile ? relInProject(mainFile) : null,
    fileNameRef,
    selectRef: handleSelectRef,
    viewportRef,
    setLog,
  });
  const { lastReplace, runSearch, runPreview, runApply, undoReplace } = useProjectReplace({
    projectId,
    root,
    mainFile,
    relInProject,
    buffersRef,
    setBuffers,
    setTex,
    fileNameRef,
  });
  const {
    currentLine,
    setCurrentLine,
    synctexFlash,
    pageNumber,
    setPageNumber,
    handleForwardSync,
    handleInverseSync,
  } = useSynctex({
    pdfUrl,
    source: previewDoc?.source ?? null,
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
  });
  const outline = useOutline(tex, fileName);

  const handleTexChange = useCallback(
    (v: string) => {
      const t0 = performance.now();
      setTex(v);
      if (hasDir(fileName)) {
        setBuffers((b) => updateBuffer(b, fileName, v));
      }
      // Keystroke-to-paint probe emission.
      requestAnimationFrame(() => {
        const dt = Math.round(performance.now() - t0);
        if (v.length > 1_000_000) {
          emit({
            scope: 'app',
            kind: 'info',
            actor: 'system',
            message: `editor render ${(v.length / 1_048_576).toFixed(1)}MB file in ${dt}ms`,
            event: { action: 'editor.render', bytes: v.length, ms: dt },
          });
        }
      });
    },
    [fileName, setBuffers],
  );

  // ---- Command registry binding ----
  // Menus, icon buttons, and chords invoke these actions. Rename uses the
  // same project-file predicate the registry gates on.
  const isProjectFile = (p: string) => root != null && hasDir(p) && relUnder(root, p) !== null;
  const compileTarget = mainFile ?? (hasDir(fileName) ? fileName : null);
  const workingLabel = (() => {
    const t = compileTarget ?? largeFile ?? fileName;
    const base = baseName(t) || t;
    return relOf(t) === t ? base : `${relOf(t)}`;
  })();
  const widgetApproval = useWidgetApproval({
    projectId,
    mainRel: mainFile ? relInProject(mainFile) : null,
  });
  const exporter = useExport({
    pdf: previewDoc?.source ?? null,
    project: root && projectId ? { rootId: projectId, path: root } : null,
  });
  const menuCtx: MenuContext = {
    hasProject: root != null,
    isProjectFile: isProjectFile(fileName),
    dirty: !!buffers.get(fileName)?.dirty,
    compiling: compilePhase === 'compiling',
    autoCompile,
    precheckCount: precheck?.findings.length ?? 0,
    pdfOpen: pdfUrl != null,
    editorReady: viewportRef.current != null && largeFile == null,
    view: shell.view,
    preset: shell.preset,
    logCollapsed,
    outlineVisible: shell.outlineVisible,
    // Selection submenus navigate sections only.
    outlineLines: outline.sections,
    outlinePicks: outline.picks,
    canUndoDelete: trash.size > 0,
    canUndoReplace: lastReplace != null,
    historyAvailable:
      historyAvailability({
        hasProject: root != null,
        relPath: relInProject(fileName),
        storeReady: projectId != null,
      }) === 'ready',
    reloadPending: conflicts.length > 0,
    theme: themeMode,
    density,
    recentProjects,
  };
  const menuActions = commandActions({
    active: { path: fileName, inProject: isProjectFile(fileName), root },
    shell,
    appearance: { setTheme: onThemeMode, setDensity: onDensityMode },
    project: {
      open,
      openRoot,
      openWelcome,
      setRecentProjects,
      findInProject: () => {
        shell.setTreeVisible(true);
        setSearchOpen(true);
        setSearchFocus((k) => k + 1);
      },
      undoReplace,
    },
    files: {
      handleCreate,
      handleDelete,
      handleClean,
      handleUndo,
      closeBuffer: handleCloseBuffer,
      save,
      setMain: setMainToOpenFile,
      resolveExternal,
      openHistory: history.openHistory,
    },
    editor: {
      viewport: () => viewportRef.current,
      goToDefinition: () => nav.goToDefinitionRef.current(),
      pickSection: setCurrentLine,
      togglePick: outline.togglePick,
    },
    compile: {
      compile: () => compileRef.current(),
      handleCompileFile,
      makeOffline,
      openPrecheck,
      autoCompile,
      setAutoCompile,
      forwardSync: () => forwardSyncRef.current(),
      zoom: (a) => zoomActionRef.current?.(a),
    },
    exporter,
    dialogs: { ...dialogs, widgets: widgetApproval.openPanel },
  });
  const menuSections = buildMenus(menuCtx, menuActions);
  useDevCamera((m) => {
    if (m.op === 'open') {
      if (root) void handleSelectRef.current(joinPath(root, m.rel));
    } else if (m.op === 'line') viewportRef.current?.goToLine(m.line);
    else if (m.op === 'page') setPageNumber(m.page);
    else menuActionRef.current(m.id);
  });
  menuActionRef.current = (id: string) => {
    for (const sec of menuSections) {
      const cmd = sec.commands.find((c) => c.id === id);
      if (cmd && cmd.enabled && cmd.visible !== false) {
        void cmd.run?.();
        return;
      }
    }
  };

  const articleSync = useArticleSync({
    mainFile,
    root,
    projectId,
    previewStamp: previewDoc?.stamp ?? null,
    compiling: compilePhase === 'compiling',
    currentLine,
    sections: outline.sections,
    widgets: widgetApproval,
  });
  const { article, mainDoc } = articleSync;

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', height: '100vh' }}>
      <MenuBar
        sections={menuSections}
        status={
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
            <Typography variant="caption" color="text.secondary">
              {compilePhase === 'compiling' ? `compiling ${compileTimer}s` : compilePhase}
            </Typography>
            <CompileButton
              targetLabel={workingLabel}
              compiling={compilePhase === 'compiling'}
              onCompile={() => menuActionRef.current('tools.compile')}
              onCancel={() => menuActionRef.current('tools.cancel')}
            />
          </Box>
        }
      />
      <WorkArea
        shell={shell}
        side={
          <SideColumn
            root={root}
            searchOpen={searchOpen}
            search={{
              runSearch,
              onOpenHit: nav.openHit,
              runPreview,
              runApply,
              lastReplace,
              onUndoReplace: () => void undoReplace(),
              onClose: () => setSearchOpen(false),
              focusKey: searchFocus,
            }}
            tree={{
              tree,
              selected: fileName,
              onSelect: handleSelect,
              onDoubleClick: (p) => {
                if (p.endsWith('.tex')) void selectAndSetMain(p);
              },
              onDelete: handleDelete,
              onSetMain: (p) => {
                void selectAndSetMain(p);
              },
              onCompileFile: (p) => {
                void handleCompileFile(p);
              },
              onCreate: handleCreate,
              onRename: handleRename,
              onExpandDir: expandDir,
              rootDir: root,
              mainFile,
              lazy: true,
              maxDepth: 2,
              filterHidden: true,
            }}
            outlineVisible={shell.outlineVisible}
            outline={{ entries: outline.entries, onJump: (line) => setCurrentLine(line) }}
          />
        }
        editor={
          <EditorPane
            fileName={fileName}
            fileLabel={relOf(fileName) ?? fileName}
            dirty={!!buffers.get(fileName)?.dirty}
            mainLabel={relOf(mainFile) ?? '(none)'}
            mainTip={mainFileTip(mainFile, mainSource)}
            mainFile={mainFile}
            mainCandidates={mainCandidates}
            onPickMain={(c) => void pickMain(c)}
            labelOf={(c) => relOf(c) ?? c}
            tabs={{
              buffers,
              active: fileName,
              onSelect: (p) => {
                void handleSelect(p);
              },
              onClose: (p) => {
                void handleCloseBuffer(p);
              },
              onCloseOthers: (k) => {
                void handleCloseOthers(k);
              },
              onCloseAll: () => {
                void handleCloseAll();
              },
            }}
            largeFile={largeFile}
            previewFile={previewFile}
            viewport={{
              value: tex,
              onChange: handleTexChange,
              onSave: save,
              line: currentLine,
              flashKey: synctexFlash,
              select: nav.reveal && nav.reveal.path === fileName ? nav.reveal : undefined,
              viewportRef,
              onDoubleClickRef: forwardSyncLineRef,
              filePath: fileName,
              prefs,
              definitionRef: nav.definitionRef,
            }}
            log={log}
            logTitle={compileLogTitle(log, compiled)}
          />
        }
        preview={
          <PreviewPane
            pdfUrl={pdfUrl}
            stamp={previewDoc?.stamp ?? 0}
            mainSource={mainDoc}
            pageNumber={pageNumber}
            onPage={setPageNumber}
            onSync={handleForwardSync}
            onInverse={(page, x, y) => {
              void handleInverseSync(page, x, y);
            }}
            syncDisabled={compilePhase === 'compiling'}
            zoomActionRef={zoomActionRef}
            compiling={compilePhase === 'compiling'}
            articleHtml={article.html}
            articleLoading={article.loading}
            articleError={article.error}
            canLoadArticle={mainDoc != null && pdfUrl != null}
            onLoadArticle={articleSync.loadNow}
            articleAnchor={articleSync.anchor}
            onArticleSync={articleSync.onSync}
            onApproveArticle={articleSync.onApproveRequest}
            approvalText={articleSync.approvalText}
            onReviewApprovals={widgetApproval.openPanel}
          />
        }
      />
      <LogStream
        height={layout.logHeight}
        onHeight={shell.setLogHeight}
        collapsed={logCollapsed}
        onToggleCollapse={shell.toggleLog}
        onJump={handleProblemJump}
      />
      <StatusBar
        mainFile={relOf(mainFile)}
        mainFileTitle={mainFile}
        historyCount={trash.size}
        revisionCount={history.revisionCount}
        revisionTitle={`${history.revisionCount} revision${history.revisionCount === 1 ? '' : 's'} of ${
          relInProject(fileName) ?? fileName
        } — click to open History`}
        onOpenHistory={() => void history.openHistory()}
        phase={compilePhase}
        timer={compileTimer}
        message={compilePhase === 'compiling' ? (progress ?? log) : log}
        messageTitle={compilePhase === 'compiling' ? undefined : compileLogTitle(log, compiled)}
        offline={compilePhase === 'compiling' ? null : offline}
        warnings={precheck?.findings.length ?? 0}
        onOpenWarnings={openPrecheck}
      />
      <AppDialogs
        dialogs={dialogs}
        file={{
          path: fileName,
          rel: relInProject(fileName),
          line: currentLine,
          rename: (name) => void handleRename(fileName, name),
          goToLine: (n) => viewportRef.current?.goToLine(n),
        }}
        palette={{
          findFiles: nav.findFiles,
          rank: nav.rankNames,
          commands: paletteCommands(menuSections, ['file.quick-open', 'view.command-palette']),
          onOpenFile: nav.openFileRel,
        }}
        settings={{ prefs, onPrefs, precheckPopup, onPrecheckPopup: setPrecheckPopup }}
        history={history}
        exporter={exporter}
        widgets={widgetApproval}
        templates={{
          project: root && projectId ? { rootId: projectId, path: root } : null,
          mainRel: mainFile ? relInProject(mainFile) : null,
          openRoot: (r, main) => openRoot(r, { main }),
        }}
        conflicts={{
          labels: conflicts.map((c) => relInProject(c) ?? c),
          resolve: (c) => void resolveExternal(c),
        }}
        precheck={{
          findings: precheck,
          open: precheckOpen,
          popup: precheckPopup,
          close: closePrecheck,
          onJump: nav.openFinding,
        }}
        openForAgent={openForAgent}
      />
    </Box>
  );
}
