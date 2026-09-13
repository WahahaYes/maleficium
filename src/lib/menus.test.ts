import { describe, it, expect } from 'vitest';
import { buildMenus, type MenuContext, type CommandActions } from './commands';
import { KEYMAP } from './keymap';

const baseCtx: MenuContext = {
  hasProject: true, isProjectFile: true, dirty: false, compiling: false,
  pdfOpen: true, view: { tree: true, editor: true, preview: true }, preset: 'both',
  logCollapsed: false, outlineVisible: true, canUndoDelete: true,
  reloadPending: false, theme: 'dark',
};
const noop = () => {};
const actions: CommandActions = {
  openProject: noop, newFile: noop, closeFile: noop, save: noop, setMainFile: noop,
  reloadFromDisk: noop, keepMine: noop, clean: noop, undoDelete: noop, renameActive: noop,
  deleteActive: noop, selectAll: noop, expandSelection: noop, shrinkSelection: noop,
  goToLine: noop, setPreset: noop, toggleTree: noop, togglePreview: noop, toggleLog: noop,
  toggleOutline: noop, setTheme: noop, compile: noop, cancelCompile: noop, forwardSync: noop,
  inverseHint: noop, gitStatus: noop, gitShowHead: noop, showShortcuts: noop, showAbout: noop,
};

describe('command registry', () => {
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
