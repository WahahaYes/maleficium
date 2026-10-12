import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('./events', () => ({ emit: vi.fn() }));
vi.mock('./compile', () => ({ cancelCompile: vi.fn(() => Promise.resolve()) }));
vi.mock('./recentProjects', () => ({ pruneRecentProjects: vi.fn() }));

import { commandActions, type ActionGroups } from './commandActions';
import type { CommandActions } from './commands';
import { emit } from './events';
import { cancelCompile } from './compile';
import { pruneRecentProjects } from './recentProjects';

function groups(over: Partial<ActionGroups['active']> = {}) {
  const viewport = {
    openFind: vi.fn(),
    selectAll: vi.fn(),
    expandSelection: vi.fn(),
    shrinkSelection: vi.fn(),
  };
  const g = {
    active: { path: '/p/a.tex', inProject: true, root: '/p', ...over },
    shell: {
      setPreset: vi.fn(),
      setTreeVisible: vi.fn(),
      toggleTree: vi.fn(),
      togglePreview: vi.fn(),
      toggleLog: vi.fn(),
      toggleOutline: vi.fn(),
    },
    appearance: { setTheme: vi.fn(), setDensity: vi.fn() },
    project: {
      open: vi.fn(),
      openRoot: vi.fn(),
      openWelcome: vi.fn(),
      setRecentProjects: vi.fn(),
      findInProject: vi.fn(),
      undoReplace: vi.fn(),
    },
    files: {
      handleCreate: vi.fn(),
      handleDelete: vi.fn(),
      handleClean: vi.fn(),
      handleUndo: vi.fn(),
      closeBuffer: vi.fn(),
      save: vi.fn(),
      setMain: vi.fn(),
      resolveExternal: vi.fn(),
      openHistory: vi.fn(),
    },
    editor: {
      viewport: () => viewport,
      goToDefinition: vi.fn(),
      pickSection: vi.fn(),
      togglePick: vi.fn(),
    },
    compile: {
      compile: vi.fn(),
      handleCompileFile: vi.fn(),
      makeOffline: vi.fn(),
      openPrecheck: vi.fn(),
      autoCompile: true,
      setAutoCompile: vi.fn(),
      forwardSync: vi.fn(),
      zoom: vi.fn(),
    },
    exporter: {
      exportPdfAs: vi.fn(),
      exportZipAs: vi.fn(),
      exportBundleAs: vi.fn(),
      previewBundle: vi.fn(),
    },
    dialogs: { show: vi.fn(), palette: vi.fn(), template: vi.fn(), widgets: vi.fn() },
  };
  return { g, viewport, a: commandActions(g) };
}

beforeEach(() => vi.clearAllMocks());

describe('commandActions', () => {
  it('routes every action to its group', () => {
    const { g, viewport, a } = groups();
    // [action, call it, the mock it must reach, expected args]
    const cases: [keyof CommandActions, () => void, unknown, unknown[]][] = [
      ['openProject', () => a.openProject(), g.project.open, []],
      ['openRecent', () => a.openRecent('/q'), g.project.openRoot, ['/q']],
      ['showWelcome', () => a.showWelcome(), g.project.openWelcome, []],
      ['findInProject', () => a.findInProject(), g.project.findInProject, []],
      ['undoReplace', () => a.undoReplace(), g.project.undoReplace, []],
      ['quickOpen', () => a.quickOpen(), g.dialogs.palette, ['']],
      ['commandPalette', () => a.commandPalette(), g.dialogs.palette, ['>']],
      ['newFile', () => a.newFile(), g.files.handleCreate, ['/p', 'untitled.tex']],
      ['closeFile', () => a.closeFile(), g.files.closeBuffer, ['/p/a.tex']],
      ['save', () => a.save(), g.files.save, []],
      ['setMainFile', () => a.setMainFile(), g.files.setMain, ['/p/a.tex']],
      ['reloadFromDisk', () => a.reloadFromDisk(), g.files.resolveExternal, ['reload']],
      ['keepMine', () => a.keepMine(), g.files.resolveExternal, ['keep']],
      ['clean', () => a.clean(), g.files.handleClean, []],
      ['undoDelete', () => a.undoDelete(), g.files.handleUndo, []],
      ['showHistory', () => a.showHistory(), g.files.openHistory, []],
      ['renameActive', () => a.renameActive(), g.dialogs.show, ['rename']],
      ['deleteActive', () => a.deleteActive(), g.files.handleDelete, ['/p/a.tex']],
      ['findInFile', () => a.findInFile(), viewport.openFind, []],
      ['selectAll', () => a.selectAll(), viewport.selectAll, []],
      ['expandSelection', () => a.expandSelection(), viewport.expandSelection, []],
      ['shrinkSelection', () => a.shrinkSelection(), viewport.shrinkSelection, []],
      ['goToDefinition', () => a.goToDefinition(), g.editor.goToDefinition, []],
      ['goToLine', () => a.goToLine(), g.dialogs.show, ['goToLine']],
      ['pickOutlineSection', () => a.pickOutlineSection(7), g.editor.pickSection, [7]],
      ['toggleOutlinePick', () => a.toggleOutlinePick(7), g.editor.togglePick, [7]],
      ['setPreset', () => a.setPreset('editor'), g.shell.setPreset, ['editor']],
      ['toggleTree', () => a.toggleTree(), g.shell.toggleTree, []],
      ['togglePreview', () => a.togglePreview(), g.shell.togglePreview, []],
      ['toggleLog', () => a.toggleLog(), g.shell.toggleLog, []],
      ['toggleOutline', () => a.toggleOutline(), g.shell.toggleOutline, []],
      ['setTheme', () => a.setTheme('light'), g.appearance.setTheme, ['light']],
      ['setDensity', () => a.setDensity('compact'), g.appearance.setDensity, ['compact']],
      ['zoomPreview', () => a.zoomPreview('in'), g.compile.zoom, ['in']],
      ['showSettings', () => a.showSettings(), g.dialogs.show, ['settings']],
      ['showWidgets', () => a.showWidgets(), g.dialogs.widgets, []],
      ['compile', () => a.compile(), g.compile.compile, []],
      ['compileFile', () => a.compileFile(), g.compile.handleCompileFile, ['/p/a.tex']],
      ['makeOffline', () => a.makeOffline(), g.compile.makeOffline, []],
      ['showPrecheck', () => a.showPrecheck(), g.compile.openPrecheck, []],
      ['toggleAutoCompile', () => a.toggleAutoCompile(), g.compile.setAutoCompile, [false]],
      ['forwardSync', () => a.forwardSync(), g.compile.forwardSync, []],
      ['cancelCompile', () => a.cancelCompile(), cancelCompile, []],
      ['exportPdf', () => a.exportPdf(), g.exporter.exportPdfAs, []],
      ['exportZip', () => a.exportZip(), g.exporter.exportZipAs, []],
      ['exportBundleFolder', () => a.exportBundleFolder(), g.exporter.exportBundleAs, ['folder']],
      [
        'exportBundleSingleFile',
        () => a.exportBundleSingleFile(),
        g.exporter.exportBundleAs,
        ['single-file'],
      ],
      ['previewInBrowser', () => a.previewInBrowser(), g.exporter.previewBundle, []],
      ['newFromTemplate', () => a.newFromTemplate(), g.dialogs.template, ['gallery']],
      ['saveAsTemplate', () => a.saveAsTemplate(), g.dialogs.template, ['save']],
      ['importTemplate', () => a.importTemplate(), g.dialogs.template, ['import']],
      ['clearRecents', () => a.clearRecents(), g.project.setRecentProjects, [[]]],
      ['showShortcuts', () => a.showShortcuts(), g.dialogs.show, ['shortcuts']],
      ['showAbout', () => a.showAbout(), g.dialogs.show, ['about']],
    ];
    expect(new Set(cases.map((c) => c[0]))).toEqual(new Set(Object.keys(a)));
    for (const [name, run, mock, args] of cases) {
      vi.clearAllMocks();
      run();
      expect(mock, name).toHaveBeenCalledWith(...args);
    }
    expect(pruneRecentProjects).not.toHaveBeenCalled();
    a.clearRecents();
    expect(pruneRecentProjects).toHaveBeenCalledOnce();
  });

  it('blocks New File without a project and Rename outside it', () => {
    const { g, a } = groups({ root: null, inProject: false });
    a.newFile();
    a.renameActive();
    expect(g.files.handleCreate).not.toHaveBeenCalled();
    expect(g.dialogs.show).not.toHaveBeenCalled();
    expect(vi.mocked(emit).mock.calls.map(([e]) => e.event)).toEqual([
      { action: 'command.blocked', command: 'newFile', reason: 'no-project' },
      { action: 'command.blocked', command: 'renameActive', reason: 'not-a-project-file' },
    ]);
  });

  it('Selection commands no-op before the editor mounts', () => {
    const { g } = groups();
    const a = commandActions({ ...g, editor: { ...g.editor, viewport: () => null } });
    expect(() => [a.selectAll(), a.findInFile()]).not.toThrow();
  });
});
