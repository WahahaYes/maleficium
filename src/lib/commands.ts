// commands.ts — every user-invokable action, registered once with a stable
// namespaced id. Menus, buttons, chords, and tools invoke these entries;
// no handler lives only inside a component.
//
// Ids are `domain.verb-noun`; labels are Title Case (`…` iff dialog); every
// `accelerator` exists in the keymap; destructive commands confirm via
// dialog; every visible row runs.

import type { ZoomAction } from './zoom';
export type CommandId =
  | 'file.open-project'
  | 'file.new-file'
  | 'file.close-file'
  | 'file.save'
  | 'file.set-main'
  | 'file.new-from-template'
  | 'file.save-as-template'
  | 'file.import-template'
  | 'file.export-pdf'
  | 'file.export-zip'
  | 'file.reload'
  | 'file.keep-mine'
  | 'file.clean'
  | 'file.recent'
  | 'file.recent-clear'
  | 'edit.undo-delete'
  | 'edit.find'
  | 'search.find-in-project'
  | 'search.undo-replace'
  | 'history.show'
  | 'file.quick-open'
  | 'view.command-palette'
  | 'edit.rename'
  | 'edit.delete'
  | 'selection.select-all'
  | 'selection.expand'
  | 'selection.shrink'
  | 'selection.go-to-line'
  | 'selection.go-to-definition'
  | 'selection.pick-one'
  | 'selection.pick-many'
  | 'view.layout'
  | 'view.preset-both'
  | 'view.preset-editor'
  | 'view.preset-preview'
  | 'view.toggle-tree'
  | 'view.toggle-preview'
  | 'view.toggle-log'
  | 'view.toggle-outline'
  | 'view.theme'
  | 'view.theme-dark'
  | 'view.theme-light'
  | 'view.density'
  | 'view.density-comfortable'
  | 'view.density-compact'
  | 'view.zoom'
  | 'view.zoom-in'
  | 'view.zoom-out'
  | 'view.zoom-fit-width'
  | 'view.zoom-fit-page'
  | 'appearance.settings'
  | 'tools.compile'
  | 'tools.compile-file'
  | 'tools.make-offline'
  | 'tools.precheck-warnings'
  | 'tools.auto-compile'
  | 'tools.cancel'
  | 'tools.forward-sync'
  | 'help.shortcuts'
  | 'help.about'
  | 'help.welcome';

export type ViewPreset = 'both' | 'editor' | 'preview' | 'custom';
export type ViewState = { tree: boolean; editor: boolean; preview: boolean };

export function presetOf(v: ViewState): ViewPreset {
  if (v.tree && v.editor && v.preview) return 'both';
  if (!v.tree && v.editor && !v.preview) return 'editor';
  if (!v.tree && !v.editor && v.preview) return 'preview';
  return 'custom';
}

import type { Density } from './theme';
import { baseName } from './paths';

export interface MenuContext {
  hasProject: boolean;
  /** Active file lives inside the project (enables Set-main/Rename/Delete). */
  isProjectFile: boolean;
  dirty: boolean;
  compiling: boolean;
  /** Compile on save is on. */
  autoCompile: boolean;
  /** Pre-compile findings of the latest run. */
  precheckCount: number;
  pdfOpen: boolean;
  /** Live editor mounted (viewport bridge assigned) — Selection enabled. */
  editorReady: boolean;
  view: ViewState;
  preset: ViewPreset;
  logCollapsed: boolean;
  outlineVisible: boolean;
  /** Outline section lines for the Go to Section and Pick Sections submenus. */
  outlineLines: { line: number; title: string }[];
  /** Currently multi-picked outline lines. */
  outlinePicks: number[];
  canUndoDelete: boolean;
  /** The last project replace can still be undone. */
  canUndoReplace: boolean;
  /** Revisions are listable for the active file (project file, store reachable). */
  historyAvailable: boolean;
  reloadPending: boolean;
  theme: 'dark' | 'light';
  density: Density;
  /** Most-recent-first project roots for File > Open Recent. */
  recentProjects: string[];
}

export interface CommandActions {
  openProject: () => void;
  newFile: () => void;
  closeFile: () => void;
  save: () => void;
  setMainFile: () => void;
  reloadFromDisk: () => void;
  keepMine: () => void;
  clean: () => void;
  undoDelete: () => void;
  findInFile: () => void;
  findInProject: () => void;
  quickOpen: () => void;
  commandPalette: () => void;
  undoReplace: () => void;
  showHistory: () => void;
  showSettings: () => void;
  renameActive: () => void;
  deleteActive: () => void;
  selectAll: () => void;
  expandSelection: () => void;
  shrinkSelection: () => void;
  goToLine: () => void;
  goToDefinition: () => void;
  /** Choose-1-from-N: jump to one outline section. */
  pickOutlineSection: (line: number) => void;
  /** Choose-N: toggle outline entries as a multi-pick set. */
  toggleOutlinePick: (line: number) => void;
  setPreset: (p: Exclude<ViewPreset, 'custom'>) => void;
  toggleTree: () => void;
  togglePreview: () => void;
  toggleLog: () => void;
  toggleOutline: () => void;
  setTheme: (m: 'dark' | 'light') => void;
  setDensity: (d: Density) => void;
  zoomPreview: (a: ZoomAction) => void;
  openRecent: (root: string) => void;
  clearRecents: () => void;
  compile: () => void;
  compileFile: () => void;
  makeOffline: () => void;
  showPrecheck: () => void;
  toggleAutoCompile: () => void;
  exportPdf: () => void;
  newFromTemplate: () => void;
  saveAsTemplate: () => void;
  importTemplate: () => void;
  showWelcome: () => void;
  exportZip: () => void;
  cancelCompile: () => void;
  forwardSync: () => void;
  showShortcuts: () => void;
  showAbout: () => void;
}

export interface MenuCommand {
  id: CommandId;
  label: string;
  accelerator?: string;
  checked?: boolean;
  enabled: boolean;
  visible?: boolean;
  /** Nested submenu — renders a flyout instead of running an action. */
  children?: MenuCommand[];
  run?: () => void | Promise<void>;
}

export interface MenuSection {
  id: string;
  title: string;
  commands: MenuCommand[];
}

export function buildMenus(ctx: MenuContext, a: CommandActions): MenuSection[] {
  // Outline submenus cap at 25 rows; the full outline lives in the tree column.
  const PICK_CAP = 25;
  return [
    {
      id: 'file',
      title: 'File',
      commands: [
        {
          id: 'file.open-project',
          label: 'Open Project…',
          accelerator: 'Ctrl+O',
          enabled: true,
          run: a.openProject,
        },
        {
          id: 'file.recent',
          label: 'Open Recent',
          enabled: ctx.recentProjects.length > 0,
          children: [
            ...ctx.recentProjects.map((r) => ({
              id: 'file.recent' as const,
              label: baseName(r) || r,
              enabled: true,
              run: () => a.openRecent(r),
            })),
            ...(ctx.recentProjects.length > 0
              ? [
                  {
                    id: 'file.recent-clear' as const,
                    label: 'Clear Recents',
                    enabled: true,
                    run: a.clearRecents,
                  },
                ]
              : []),
          ],
        },
        {
          id: 'file.quick-open',
          label: 'Go to File…',
          accelerator: 'Ctrl+P',
          enabled: ctx.hasProject,
          run: a.quickOpen,
        },
        { id: 'file.new-file', label: 'New File…', enabled: ctx.hasProject, run: a.newFile },
        {
          id: 'file.new-from-template',
          label: 'New Project from Template…',
          enabled: true,
          run: a.newFromTemplate,
        },
        {
          id: 'file.save-as-template',
          label: 'Save Project as Template…',
          enabled: ctx.hasProject,
          run: a.saveAsTemplate,
        },
        {
          id: 'file.import-template',
          label: 'Import Folder as Template…',
          enabled: true,
          run: a.importTemplate,
        },
        {
          id: 'file.close-file',
          label: 'Close File',
          accelerator: 'Ctrl+W',
          enabled: ctx.dirty || ctx.hasProject,
          run: a.closeFile,
        },
        { id: 'file.save', label: 'Save', accelerator: 'Ctrl+S', enabled: true, run: a.save },
        { id: 'file.export-pdf', label: 'Export PDF…', enabled: ctx.pdfOpen, run: a.exportPdf },
        {
          id: 'file.export-zip',
          label: 'Export Project as Zip…',
          enabled: ctx.hasProject,
          run: a.exportZip,
        },
        {
          id: 'file.set-main',
          label: 'Set as Main File',
          enabled: ctx.isProjectFile,
          run: a.setMainFile,
        },
        {
          id: 'file.reload',
          label: 'Reload from Disk',
          enabled: true,
          visible: ctx.reloadPending,
          run: a.reloadFromDisk,
        },
        {
          id: 'file.keep-mine',
          label: 'Keep Mine',
          enabled: true,
          visible: ctx.reloadPending,
          run: a.keepMine,
        },
        { id: 'file.clean', label: 'Clean Build Output', enabled: ctx.hasProject, run: a.clean },
      ],
    },
    {
      id: 'edit',
      title: 'Edit',
      commands: [
        {
          id: 'edit.undo-delete',
          label: 'Undo Delete',
          enabled: ctx.canUndoDelete,
          run: a.undoDelete,
        },
        {
          id: 'edit.find',
          label: 'Find…',
          accelerator: 'Ctrl+F',
          enabled: ctx.editorReady,
          run: a.findInFile,
        },
        {
          id: 'search.find-in-project',
          label: 'Find in Project…',
          accelerator: 'Ctrl+Shift+F',
          enabled: ctx.hasProject,
          run: a.findInProject,
        },
        {
          id: 'search.undo-replace',
          label: 'Undo Replace',
          enabled: ctx.canUndoReplace,
          run: a.undoReplace,
        },
        {
          id: 'history.show',
          label: 'File History…',
          accelerator: 'Ctrl+H',
          enabled: ctx.historyAvailable,
          run: a.showHistory,
        },
        { id: 'edit.rename', label: 'Rename…', enabled: ctx.isProjectFile, run: a.renameActive },
        { id: 'edit.delete', label: 'Delete', enabled: ctx.isProjectFile, run: a.deleteActive },
      ],
    },
    {
      id: 'selection',
      title: 'Selection',
      commands: [
        {
          id: 'selection.select-all',
          label: 'Select All',
          accelerator: 'Ctrl+A',
          enabled: ctx.editorReady,
          run: a.selectAll,
        },
        {
          id: 'selection.expand',
          label: 'Expand Selection',
          accelerator: 'Shift+Alt+Right',
          enabled: ctx.editorReady,
          run: a.expandSelection,
        },
        {
          id: 'selection.shrink',
          label: 'Shrink Selection',
          accelerator: 'Shift+Alt+Left',
          enabled: ctx.editorReady,
          run: a.shrinkSelection,
        },
        {
          id: 'selection.go-to-line',
          label: 'Go to Line…',
          accelerator: 'Ctrl+G',
          enabled: ctx.editorReady,
          run: a.goToLine,
        },
        {
          id: 'selection.go-to-definition',
          label: 'Go to Definition',
          accelerator: 'F12',
          enabled: ctx.editorReady && ctx.hasProject,
          run: a.goToDefinition,
        },
        {
          id: 'selection.pick-one',
          label: 'Go to Section…',
          enabled: ctx.editorReady && ctx.outlineLines.length > 0,
          children: ctx.outlineLines.slice(0, PICK_CAP).map((o) => ({
            id: 'selection.pick-one' as const,
            label: `${o.line}: ${o.title}`.slice(0, 60),
            enabled: true,
            run: () => a.pickOutlineSection(o.line),
          })),
        },
        {
          id: 'selection.pick-many',
          label: `Pick Sections${ctx.outlinePicks.length > 0 ? ` (${ctx.outlinePicks.length})` : ''}…`,
          enabled: ctx.editorReady && ctx.outlineLines.length > 0,
          children:
            ctx.outlinePicks.length > 0
              ? ctx.outlineLines.slice(0, PICK_CAP).map((o) => ({
                  id: 'selection.pick-many' as const,
                  label:
                    `${ctx.outlinePicks.includes(o.line) ? '✓ ' : ''}${o.line}: ${o.title}`.slice(
                      0,
                      62,
                    ),
                  checked: ctx.outlinePicks.includes(o.line),
                  enabled: true,
                  run: () => a.toggleOutlinePick(o.line),
                }))
              : ctx.outlineLines.slice(0, PICK_CAP).map((o) => ({
                  id: 'selection.pick-many' as const,
                  label: `${o.line}: ${o.title}`.slice(0, 60),
                  enabled: true,
                  run: () => a.toggleOutlinePick(o.line),
                })),
        },
      ],
    },
    {
      id: 'view',
      title: 'View',
      commands: [
        {
          id: 'view.command-palette',
          label: 'Command Palette…',
          accelerator: 'Ctrl+Shift+P',
          enabled: true,
          run: a.commandPalette,
        },
        {
          id: 'view.layout',
          label: `Layout: ${ctx.preset === 'both' ? 'Editor + Preview' : ctx.preset === 'editor' ? 'Editor Only' : ctx.preset === 'preview' ? 'Preview Only' : 'Custom'}`,
          enabled: true,
          children: (['both', 'editor', 'preview'] as const).map((p) => ({
            id: (
              {
                both: 'view.preset-both',
                editor: 'view.preset-editor',
                preview: 'view.preset-preview',
              } as const
            )[p],
            label:
              p === 'both' ? 'Editor + Preview' : p === 'editor' ? 'Editor Only' : 'Preview Only',
            checked: ctx.preset === p,
            enabled: true,
            run: () => a.setPreset(p),
          })),
        },
        {
          id: 'view.toggle-tree',
          label: 'File Tree',
          accelerator: 'Ctrl+B',
          checked: ctx.view.tree,
          enabled: true,
          run: a.toggleTree,
        },
        {
          id: 'view.toggle-preview',
          label: 'Preview Pane',
          checked: ctx.view.preview,
          enabled: true,
          run: a.togglePreview,
        },
        {
          id: 'view.toggle-log',
          label: 'Log Stream',
          checked: !ctx.logCollapsed,
          enabled: true,
          run: a.toggleLog,
        },
        {
          id: 'view.toggle-outline',
          label: 'Outline',
          checked: ctx.outlineVisible,
          enabled: true,
          run: a.toggleOutline,
        },
        {
          id: 'view.theme',
          label: `Theme: ${ctx.theme === 'dark' ? 'Dark' : 'Light'}`,
          enabled: true,
          children: [
            {
              id: 'view.theme-dark',
              label: 'Dark',
              checked: ctx.theme === 'dark',
              enabled: true,
              run: () => a.setTheme('dark'),
            },
            {
              id: 'view.theme-light',
              label: 'Light',
              checked: ctx.theme === 'light',
              enabled: true,
              run: () => a.setTheme('light'),
            },
          ],
        },
        {
          id: 'view.density',
          label: `Density: ${ctx.density === 'compact' ? 'Compact' : 'Comfortable'}`,
          enabled: true,
          children: [
            {
              id: 'view.density-comfortable',
              label: 'Comfortable',
              checked: ctx.density === 'comfortable',
              enabled: true,
              run: () => a.setDensity('comfortable'),
            },
            {
              id: 'view.density-compact',
              label: 'Compact',
              checked: ctx.density === 'compact',
              enabled: true,
              run: () => a.setDensity('compact'),
            },
          ],
        },
        {
          id: 'view.zoom',
          label: 'Zoom',
          enabled: ctx.pdfOpen,
          children: [
            {
              id: 'view.zoom-in',
              label: 'Zoom In',
              accelerator: 'Ctrl+=',
              enabled: ctx.pdfOpen,
              run: () => a.zoomPreview('in'),
            },
            {
              id: 'view.zoom-out',
              label: 'Zoom Out',
              accelerator: 'Ctrl+-',
              enabled: ctx.pdfOpen,
              run: () => a.zoomPreview('out'),
            },
            {
              id: 'view.zoom-fit-width',
              label: 'Fit Width',
              accelerator: 'Ctrl+0',
              enabled: ctx.pdfOpen,
              run: () => a.zoomPreview('fit-width'),
            },
            {
              id: 'view.zoom-fit-page',
              label: 'Fit Page',
              enabled: ctx.pdfOpen,
              run: () => a.zoomPreview('fit-page'),
            },
          ],
        },
        {
          id: 'appearance.settings',
          label: 'Settings…',
          enabled: true,
          run: a.showSettings,
        },
      ],
    },
    {
      id: 'tools',
      title: 'Tools',
      commands: [
        {
          id: 'tools.compile',
          label: 'Compile',
          accelerator: 'Ctrl+R',
          enabled: !ctx.compiling,
          run: a.compile,
        },
        {
          id: 'tools.compile-file',
          label: 'Compile This File',
          enabled: !ctx.compiling && ctx.isProjectFile,
          run: a.compileFile,
        },
        {
          id: 'tools.auto-compile',
          label: 'Auto-Compile on Save',
          checked: ctx.autoCompile,
          enabled: true,
          run: a.toggleAutoCompile,
        },
        {
          id: 'tools.make-offline',
          label: 'Make Available Offline',
          enabled: !ctx.compiling && ctx.hasProject,
          run: a.makeOffline,
        },
        {
          id: 'tools.precheck-warnings',
          label: 'Show Pre-compile Warnings',
          enabled: ctx.precheckCount > 0,
          run: a.showPrecheck,
        },
        {
          id: 'tools.cancel',
          label: 'Cancel Compile',
          enabled: ctx.compiling,
          run: a.cancelCompile,
        },
        {
          id: 'tools.forward-sync',
          label: 'Forward SyncTeX',
          accelerator: 'Ctrl+Alt+J',
          enabled: ctx.pdfOpen && !ctx.compiling,
          run: a.forwardSync,
        },
      ],
    },
    {
      id: 'help',
      title: 'Help',
      commands: [
        {
          id: 'help.shortcuts',
          label: 'Keyboard Shortcuts',
          accelerator: '?',
          enabled: true,
          run: a.showShortcuts,
        },
        { id: 'help.welcome', label: 'Welcome', enabled: true, run: a.showWelcome },
        { id: 'help.about', label: 'About Maleficium', enabled: true, run: a.showAbout },
      ],
    },
  ];
}
