import { describe, it, expect } from 'vitest';
import { buildMenus, type MenuContext, type CommandActions } from './commands';
import { KEYMAP } from './keymap';

const baseCtx: MenuContext = {
  hasProject: true, isProjectFile: true, dirty: false, compiling: false,
  pdfOpen: true, editorReady: true,
  view: { tree: true, editor: true, preview: true }, preset: 'both',
  logCollapsed: false, outlineVisible: true, outlineLines: [{ line: 3, title: 'Intro' }], outlinePicks: [3], canUndoDelete: true,
  reloadPending: false, theme: 'dark',
};
const noop = () => {};
const actions: CommandActions = {
  openProject: noop, newFile: noop, closeFile: noop, save: noop, setMainFile: noop,
  reloadFromDisk: noop, keepMine: noop, clean: noop, undoDelete: noop, renameActive: noop,
  deleteActive: noop, selectAll: noop, expandSelection: noop, shrinkSelection: noop,
  goToLine: noop, pickOutlineSection: noop, toggleOutlinePick: noop, setPreset: noop, toggleTree: noop, togglePreview: noop, toggleLog: noop,
  toggleOutline: noop, setTheme: noop, compile: noop, cancelCompile: noop, forwardSync: noop,
  inverseHint: noop, gitStatus: noop, gitShowHead: noop, showShortcuts: noop, showAbout: noop,
};

describe('command registry', () => {
  it('submenu hierarchy builds (pick-1 / pick-N / theme / layout)', () => {
    const all = buildMenus(baseCtx, actions).flatMap((s) => s.commands);
    const pickOne = all.find((c) => c.id === 'selection.pick-one')!;
    const pickMany = all.find((c) => c.id === 'selection.pick-many')!;
    const theme = all.find((c) => c.id === 'view.theme')!;
    const layout = all.find((c) => c.id === 'view.layout')!;
    expect(pickOne.children?.length).toBe(1);
    expect(pickMany.children?.[0].checked).toBe(true);
    expect(theme.children?.map((k) => k.id)).toEqual(['view.theme-dark', 'view.theme-light']);
    // Layout is choose-1-of-N: exactly one child checked, label echoes choice.
    expect(layout.label).toBe('Layout: Editor + Preview');
    expect(layout.children?.map((k) => k.id)).toEqual(['view.preset-both', 'view.preset-editor', 'view.preset-preview']);
    expect(layout.children?.filter((k) => k.checked).length).toBe(1);
    // Leaf ids stay unique even counting submenu children (MCP-safe).
    const leafIds = all.flatMap((c) => (c.children ? c.children.map((k) => k.id) : [c.id]));
    expect(new Set(leafIds).size).toBe(leafIds.length);
  });
  it('ids are unique across sections', () => {
    const ids = buildMenus(baseCtx, actions).flatMap((s) => s.commands.map((c) => c.id));
    expect(new Set(ids).size).toBe(ids.length);
  });
  it('every accelerator exists in KEYMAP', () => {
    const keys = new Set(KEYMAP.map((k) => k.keys));
    for (const s of buildMenus(baseCtx, actions)) {
      for (const c of s.commands) {
        if (c.accelerator) expect(keys.has(c.accelerator), c.id).toBe(true);
      }
    }
  });
  it('disabled states match context (compiling / no-project / trash-empty)', () => {
    const find = (ctx: MenuContext, id: string) =>
      buildMenus(ctx, actions).flatMap((s) => s.commands).find((c) => c.id === id)!;
    expect(find({ ...baseCtx, compiling: true }, 'tools.compile').enabled).toBe(false);
    expect(find({ ...baseCtx, compiling: true }, 'tools.cancel').enabled).toBe(true);
    expect(find({ ...baseCtx, hasProject: false }, 'file.new-file').enabled).toBe(false);
    expect(find({ ...baseCtx, canUndoDelete: false }, 'edit.undo-delete').enabled).toBe(false);
    expect(find({ ...baseCtx, reloadPending: true }, 'file.reload').visible).toBe(true);
    expect(find(baseCtx, 'file.reload').visible).toBe(false);
  });
});
