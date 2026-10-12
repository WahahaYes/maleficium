// commandActions.ts — binds the registry's CommandActions to the app. Each
// group is a slice App already holds (mostly a hook's return value, passed
// whole), so adding a command touches its group, not a flat deps list.

import type { CommandActions, ViewPreset } from './commands';
import type { Density } from './theme';
import type { ZoomAction } from './zoom';
import { cancelCompile } from './compile';
import { emit } from './events';
import type { CommandBlockedReason } from './generated/events';
import { pruneRecentProjects } from './recentProjects';

/** Modal dialogs the menus open; one shows at a time. */
export type AppDialog = 'shortcuts' | 'settings' | 'about' | 'goToLine' | 'rename';

/** The editor bridge the Selection commands drive (EditorViewportHandle). */
interface Viewport {
  openFind(): void;
  selectAll(): void;
  expandSelection(): void;
  shrinkSelection(): void;
}

type Run = () => unknown;

export interface ActionGroups {
  /** The active file, and whether it lives inside the open project. */
  active: { path: string; inProject: boolean; root: string | null };
  /** useShellLayout. */
  shell: {
    setPreset: (p: Exclude<ViewPreset, 'custom'>) => void;
    setTreeVisible: (v: boolean) => void;
    toggleTree: Run;
    togglePreview: Run;
    toggleLog: Run;
    toggleOutline: Run;
  };
  appearance: { setTheme: (m: 'dark' | 'light') => void; setDensity: (d: Density) => void };
  /** useProjectTree plus project-wide search. */
  project: {
    open: Run;
    openRoot: (root: string) => unknown;
    openWelcome: Run;
    setRecentProjects: (r: string[]) => void;
    findInProject: Run;
    undoReplace: Run;
  };
  /** Buffers, save, file ops (useFileOps), main file, history. */
  files: {
    handleCreate: (dir: string, name: string) => unknown;
    handleDelete: (path: string) => unknown;
    handleClean: Run;
    handleUndo: Run;
    closeBuffer: (path: string) => unknown;
    save: Run;
    setMain: (path: string) => unknown;
    resolveExternal: (choice: 'reload' | 'keep') => unknown;
    openHistory: Run;
  };
  editor: {
    viewport: () => Viewport | null;
    goToDefinition: Run;
    pickSection: (line: number) => void;
    togglePick: (line: number) => void;
  };
  /** useCompileRunner plus the preview's SyncTeX and zoom. */
  compile: {
    compile: Run;
    handleCompileFile: (path: string) => unknown;
    makeOffline: Run;
    openPrecheck: Run;
    autoCompile: boolean;
    setAutoCompile: (on: boolean) => void;
    forwardSync: Run;
    zoom: (a: ZoomAction) => void;
  };
  /** useExport. */
  exporter: {
    exportPdfAs: Run;
    exportZipAs: Run;
    exportBundleAs: (kind: 'folder' | 'single-file') => unknown;
    previewBundle: Run;
  };
  /** Dialogs and panels: one modal, the palette (by prefix), templates, widgets. */
  dialogs: {
    show: (d: AppDialog) => void;
    palette: (prefix: '' | '>') => void;
    template: (mode: 'gallery' | 'save' | 'import') => void;
    widgets: Run;
  };
}

function blocked(command: string, message: string, reason: CommandBlockedReason) {
  emit({
    scope: 'fs',
    kind: 'warn',
    actor: 'user',
    message,
    event: { action: 'command.blocked', command, reason },
  });
}

export function commandActions(g: ActionGroups): CommandActions {
  const { active, shell, project, files, editor, compile, exporter, dialogs } = g;
  // Menus discard results; async handlers report their own failures.
  const fire = (f: Run) => () => void f();
  return {
    openProject: fire(project.open),
    openRecent: (r) => void project.openRoot(r),
    clearRecents: () => {
      pruneRecentProjects(() => false);
      project.setRecentProjects([]);
    },
    showWelcome: fire(project.openWelcome),
    findInProject: fire(project.findInProject),
    undoReplace: fire(project.undoReplace),
    quickOpen: () => dialogs.palette(''),
    commandPalette: () => dialogs.palette('>'),

    newFile: () => {
      if (active.root) void files.handleCreate(active.root, 'untitled.tex');
      else blocked('newFile', 'New File needs an open project', 'no-project');
    },
    closeFile: () => void files.closeBuffer(active.path),
    save: fire(files.save),
    setMainFile: () => void files.setMain(active.path),
    reloadFromDisk: () => void files.resolveExternal('reload'),
    keepMine: () => void files.resolveExternal('keep'),
    clean: fire(files.handleClean),
    undoDelete: fire(files.handleUndo),
    showHistory: fire(files.openHistory),
    renameActive: () => {
      if (active.inProject) dialogs.show('rename');
      else
        blocked(
          'renameActive',
          'Rename needs a project file (open one first)',
          'not-a-project-file',
        );
    },
    deleteActive: () => void files.handleDelete(active.path),

    findInFile: () => editor.viewport()?.openFind(),
    selectAll: () => editor.viewport()?.selectAll(),
    expandSelection: () => editor.viewport()?.expandSelection(),
    shrinkSelection: () => editor.viewport()?.shrinkSelection(),
    goToDefinition: fire(editor.goToDefinition),
    goToLine: () => dialogs.show('goToLine'),
    pickOutlineSection: editor.pickSection,
    toggleOutlinePick: editor.togglePick,

    setPreset: shell.setPreset,
    toggleTree: fire(shell.toggleTree),
    togglePreview: fire(shell.togglePreview),
    toggleLog: fire(shell.toggleLog),
    toggleOutline: fire(shell.toggleOutline),
    setTheme: g.appearance.setTheme,
    setDensity: g.appearance.setDensity,
    zoomPreview: compile.zoom,
    showSettings: () => dialogs.show('settings'),
    showWidgets: fire(dialogs.widgets),

    compile: fire(compile.compile),
    compileFile: () => void compile.handleCompileFile(active.path),
    makeOffline: fire(compile.makeOffline),
    showPrecheck: fire(compile.openPrecheck),
    toggleAutoCompile: () => compile.setAutoCompile(!compile.autoCompile),
    forwardSync: fire(compile.forwardSync),
    cancelCompile: () => {
      void cancelCompile().catch((e) =>
        emit({
          scope: 'compile',
          kind: 'error',
          actor: 'user',
          message: 'cancel failed: ' + String(e).slice(0, 120),
          event: { action: 'compile.cancel-failed', error: String(e).slice(0, 200) },
        }),
      );
    },

    exportPdf: fire(exporter.exportPdfAs),
    exportZip: fire(exporter.exportZipAs),
    exportBundleFolder: () => void exporter.exportBundleAs('folder'),
    exportBundleSingleFile: () => void exporter.exportBundleAs('single-file'),
    previewInBrowser: fire(exporter.previewBundle),
    newFromTemplate: () => dialogs.template('gallery'),
    saveAsTemplate: () => dialogs.template('save'),
    importTemplate: () => dialogs.template('import'),

    showShortcuts: () => dialogs.show('shortcuts'),
    showAbout: () => dialogs.show('about'),
  };
}
