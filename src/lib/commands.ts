// commands.ts — THE source of truth for app functionality.
//
// Architecture: every user-invokable action is a direct function registered
// here with a stable namespaced id. The MenuBar, icon buttons, keyboard chords,
// and (later) the MCP/agentic sidecar all invoke the SAME registry entries —
// no handler lives only inside a component. `buildMenus(ctx, actions)` binds
// pure command shapes to App's live handlers; UI renders sections only.
//
// Rules: ids `domain.verb-noun`; labels Title Case (`…` iff dialog); every
// `accelerator` must exist in `lib/keymap.ts` KEYMAP (test asserts parity);
// destructive commands confirm via MUI dialog (never `window.confirm`);
// `legacy` marks 05-versioning-owned git UI (audit greps it); `soon` marks
// honest disabled placeholders (never fake affordances).

export type CommandId =
  | 'file.open-project' | 'file.new-file' | 'file.close-file' | 'file.save'
  | 'file.set-main' | 'file.reload' | 'file.keep-mine' | 'file.clean'
  | 'edit.undo-delete' | 'edit.rename' | 'edit.delete'
  | 'selection.select-all' | 'selection.expand' | 'selection.shrink' | 'selection.go-to-line'
  | 'selection.pick-one' | 'selection.pick-many'
  | 'view.layout' | 'view.preset-both' | 'view.preset-editor' | 'view.preset-preview'
  | 'view.toggle-tree' | 'view.toggle-preview' | 'view.toggle-log' | 'view.toggle-outline'
  | 'view.theme' | 'view.theme-dark' | 'view.theme-light'
  | 'tools.compile' | 'tools.cancel' | 'tools.forward-sync' | 'tools.inverse-hint'
  | 'tools.git-status' | 'tools.git-show-head'
  | 'help.shortcuts' | 'help.about';

export type ViewPreset = 'both' | 'editor' | 'preview' | 'custom';
export type ViewState = { tree: boolean; editor: boolean; preview: boolean };

export function presetOf(v: ViewState): ViewPreset {
  if (v.tree && v.editor && v.preview) return 'both';
  if (!v.tree && v.editor && !v.preview) return 'editor';
  if (!v.tree && !v.editor && v.preview) return 'preview';
  return 'custom';
}

export interface MenuContext {
  hasProject: boolean;
  /** Active file lives inside the project (enables Set-main/Rename/Delete). */
  isProjectFile: boolean;
  dirty: boolean;
  compiling: boolean;
  pdfOpen: boolean;
  /** Live editor mounted (viewport bridge assigned) — Selection enabled. */
  editorReady: boolean;
  view: ViewState;
  preset: ViewPreset;
  logCollapsed: boolean;
  outlineVisible: boolean;
  /** Outline section lines for the choose-1 / choose-N demo submenus. */
  outlineLines: { line: number; title: string }[];
  /** Currently multi-picked outline lines (choose-N state lives in App). */
  outlinePicks: number[];
  canUndoDelete: boolean;
  reloadPending: boolean;
  theme: 'dark' | 'light';
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
  renameActive: () => void;
  deleteActive: () => void;
  selectAll: () => void;
  expandSelection: () => void;
  shrinkSelection: () => void;
  goToLine: () => void;
  /** Choose-1-from-N: jump to one outline section (P-13 hierarchy demo). */
  pickOutlineSection: (line: number) => void;
  /** Choose-N: toggle outline entries as a multi-pick set (demo + future batch ops). */
  toggleOutlinePick: (line: number) => void;
  setPreset: (p: Exclude<ViewPreset, 'custom'>) => void;
  toggleTree: () => void;
  togglePreview: () => void;
  toggleLog: () => void;
  toggleOutline: () => void;
  setTheme: (m: 'dark' | 'light') => void;
  compile: () => void;
  cancelCompile: () => void;
  forwardSync: () => void;
  inverseHint: () => void;
  gitStatus: () => void;
  gitShowHead: () => void;
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
  /** Honest disabled placeholder — title explains itself. */
  soon?: boolean;
  /** 05-versioning-owned; do not extend. */
  legacy?: boolean;
  run?: () => void | Promise<void>;
}

export interface MenuSection {
  id: string;
  title: string;
  commands: MenuCommand[];
}

export function buildMenus(ctx: MenuContext, a: CommandActions): MenuSection[] {
  const dis = (label: string) => `${label} (soon)`;
  return [
    {
      id: 'file', title: 'File', commands: [
        { id: 'file.open-project', label: 'Open Project…', accelerator: 'Ctrl+O', enabled: true, run: a.openProject },
        { id: 'file.new-file', label: 'New File…', enabled: ctx.hasProject, run: a.newFile },
        { id: 'file.close-file', label: 'Close File', accelerator: 'Ctrl+W', enabled: ctx.dirty || ctx.hasProject, run: a.closeFile },
        { id: 'file.save', label: 'Save', accelerator: 'Ctrl+S', enabled: true, run: a.save },
        { id: 'file.set-main', label: 'Set as Main File', enabled: ctx.isProjectFile, run: a.setMainFile },
        { id: 'file.reload', label: 'Reload from Disk', enabled: true, visible: ctx.reloadPending, run: a.reloadFromDisk },
        { id: 'file.keep-mine', label: 'Keep Mine', enabled: true, visible: ctx.reloadPending, run: a.keepMine },
        { id: 'file.clean', label: 'Clean Build Output', enabled: ctx.hasProject, run: a.clean },
      ],
    },
    {
      id: 'edit', title: 'Edit', commands: [
        { id: 'edit.undo-delete', label: 'Undo Delete', enabled: ctx.canUndoDelete, run: a.undoDelete },
        { id: 'edit.rename', label: 'Rename…', enabled: ctx.isProjectFile, run: a.renameActive },
        { id: 'edit.delete', label: 'Delete', enabled: ctx.isProjectFile, run: a.deleteActive },
      ],
    },
    {
      id: 'selection', title: 'Selection', commands: [
        { id: 'selection.select-all', label: 'Select All', accelerator: 'Ctrl+A', enabled: ctx.editorReady, run: a.selectAll },
        { id: 'selection.expand', label: 'Expand Selection', accelerator: 'Shift+Alt+Right', enabled: ctx.editorReady, run: a.expandSelection },
        { id: 'selection.shrink', label: 'Shrink Selection', accelerator: 'Shift+Alt+Left', enabled: ctx.editorReady, run: a.shrinkSelection },
        { id: 'selection.go-to-line', label: 'Go to Line…', accelerator: 'Ctrl+G', enabled: ctx.editorReady, run: a.goToLine },
        {
          id: 'selection.pick-one', label: 'Go to Section…', enabled: ctx.editorReady && ctx.outlineLines.length > 0,
          children: ctx.outlineLines.slice(0, 25).map((o) => ({
            id: 'selection.pick-one' as const,
            label: `${o.line}: ${o.title}`.slice(0, 60),
            enabled: true,
            run: () => a.pickOutlineSection(o.line),
          })),
        },
        {
          id: 'selection.pick-many', label: `Pick Sections${ctx.outlinePicks.length > 0 ? ` (${ctx.outlinePicks.length})` : ''}…`, enabled: ctx.editorReady && ctx.outlineLines.length > 0,
          children: ctx.outlinePicks.length > 0
            ? ctx.outlineLines.slice(0, 25).map((o) => ({
                id: 'selection.pick-many' as const,
                label: `${ctx.outlinePicks.includes(o.line) ? '✓ ' : ''}${o.line}: ${o.title}`.slice(0, 62),
                checked: ctx.outlinePicks.includes(o.line),
                enabled: true,
                run: () => a.toggleOutlinePick(o.line),
              }))
            : ctx.outlineLines.slice(0, 25).map((o) => ({
                id: 'selection.pick-many' as const,
                label: `${o.line}: ${o.title}`.slice(0, 60),
                enabled: true,
                run: () => a.toggleOutlinePick(o.line),
              })),
        },
      ],
    },
    {
      id: 'view', title: 'View', commands: [
        {
          id: 'view.layout', label: `Layout: ${ctx.preset === 'both' ? 'Editor + Preview' : ctx.preset === 'editor' ? 'Editor Only' : ctx.preset === 'preview' ? 'Preview Only' : 'Custom'}`, enabled: true,
          children: (['both', 'editor', 'preview'] as const).map((p) => ({
            id: ({ both: 'view.preset-both', editor: 'view.preset-editor', preview: 'view.preset-preview' } as const)[p],
            label: p === 'both' ? 'Editor + Preview' : p === 'editor' ? 'Editor Only' : 'Preview Only',
            checked: ctx.preset === p,
            enabled: true,
            run: () => a.setPreset(p),
          })),
        },
        { id: 'view.toggle-tree', label: 'File Tree', accelerator: 'Ctrl+B', checked: ctx.view.tree, enabled: true, run: a.toggleTree },
        { id: 'view.toggle-preview', label: 'Preview Pane', checked: ctx.view.preview, enabled: true, run: a.togglePreview },
        { id: 'view.toggle-log', label: 'Log Stream', checked: !ctx.logCollapsed, enabled: true, run: a.toggleLog },
        { id: 'view.toggle-outline', label: 'Outline', checked: ctx.outlineVisible, enabled: true, run: a.toggleOutline },
        {
          id: 'view.theme', label: `Theme: ${ctx.theme === 'dark' ? 'Dark' : 'Light'}`, enabled: true,
          children: [
            { id: 'view.theme-dark', label: 'Dark', checked: ctx.theme === 'dark', enabled: true, run: () => a.setTheme('dark') },
            { id: 'view.theme-light', label: 'Light', checked: ctx.theme === 'light', enabled: true, run: () => a.setTheme('light') },
          ],
        },
      ],
    },
    {
      id: 'tools', title: 'Tools', commands: [
        { id: 'tools.compile', label: 'Compile', accelerator: 'Ctrl+R', enabled: !ctx.compiling, run: a.compile },
        { id: 'tools.cancel', label: 'Cancel Compile', enabled: ctx.compiling, run: a.cancelCompile },
        { id: 'tools.forward-sync', label: 'Forward SyncTeX', accelerator: 'Ctrl+Shift+F', enabled: ctx.pdfOpen && !ctx.compiling, run: a.forwardSync },
        { id: 'tools.inverse-hint', label: dis('Inverse SyncTeX'), enabled: false, soon: true, run: a.inverseHint },
        { id: 'tools.git-status', label: 'Git Status', enabled: ctx.hasProject, legacy: true, run: a.gitStatus },
        { id: 'tools.git-show-head', label: 'HEAD Diff', enabled: ctx.isProjectFile, legacy: true, run: a.gitShowHead },
      ],
    },
    {
      id: 'help', title: 'Help', commands: [
        { id: 'help.shortcuts', label: 'Keyboard Shortcuts', accelerator: '?', enabled: true, run: a.showShortcuts },
        { id: 'help.about', label: 'About Maleficium', enabled: true, run: a.showAbout },
      ],
    },
  ];
}
