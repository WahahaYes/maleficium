import { describe, it, expect } from 'vitest';
import { buildMenus, type MenuContext, type CommandActions } from './commands';
import { KEYMAP, menuChordId } from './keymap';
import { parseOutline } from './outline';

const baseCtx: MenuContext = {
  hasProject: true,
  isProjectFile: true,
  dirty: false,
  compiling: false,
  pdfOpen: true,
  editorReady: true,
  view: { tree: true, editor: true, preview: true },
  preset: 'both',
  logCollapsed: false,
  outlineVisible: true,
  outlineLines: [{ line: 3, title: 'Intro' }],
  outlinePicks: [3],
  canUndoDelete: true,
  historyAvailable: true,
  reloadPending: false,
  theme: 'dark',
  density: 'comfortable',
  recentProjects: [],
};
const noop = () => {};
const actions: CommandActions = {
  openProject: noop,
  newFile: noop,
  closeFile: noop,
  save: noop,
  setMainFile: noop,
  reloadFromDisk: noop,
  keepMine: noop,
  clean: noop,
  undoDelete: noop,
  showHistory: noop,
  showSettings: noop,
  renameActive: noop,
  deleteActive: noop,
  selectAll: noop,
  expandSelection: noop,
  shrinkSelection: noop,
  goToLine: noop,
  pickOutlineSection: noop,
  toggleOutlinePick: noop,
  setPreset: noop,
  toggleTree: noop,
  togglePreview: noop,
  toggleLog: noop,
  toggleOutline: noop,
  setTheme: noop,
  setDensity: noop,
  openRecent: noop,
  clearRecents: noop,
  compile: noop,
  compileFile: noop,
  cancelCompile: noop,
  forwardSync: noop,
  showShortcuts: noop,
  showAbout: noop,
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
    // Density submenu mirrors theme: choose-1-of-N, label echoes choice.
    const density = all.find((c) => c.id === 'view.density')!;
    expect(density.label).toBe('Density: Comfortable');
    expect(density.children?.map((k) => k.id)).toEqual([
      'view.density-comfortable',
      'view.density-compact',
    ]);
    expect(density.children?.filter((k) => k.checked).length).toBe(1);
    // Layout is choose-1-of-N: exactly one child checked, label echoes choice.
    expect(layout.label).toBe('Layout: Editor + Preview');
    expect(layout.children?.map((k) => k.id)).toEqual([
      'view.preset-both',
      'view.preset-editor',
      'view.preset-preview',
    ]);
    expect(layout.children?.filter((k) => k.checked).length).toBe(1);
    // Leaf ids stay unique even counting submenu children.
    const leafIds = all.flatMap((c) => (c.children ? c.children.map((k) => k.id) : [c.id]));
    expect(new Set(leafIds).size).toBe(leafIds.length);
  });
  it('no dead placeholder rows survive (every visible row is real)', () => {
    // Context-gated rows (Cancel while idle, Reload with nothing pending,
    // Open Recent with no recents) are honest state, not placeholders: they
    // run when their context holds. This pin only forbids PERMANENTLY dead
    // rows (no run in ANY context, `soon` label).
    const cmds = buildMenus(baseCtx, actions).flatMap((s) => s.commands);
    for (const c of cmds) {
      expect(c.label, c.id).not.toMatch(/soon/i);
      expect((c as { soon?: boolean }).soon ?? false, c.id).toBe(false);
      if (
        c.visible !== false &&
        (!c.children || c.children.length === 0) &&
        c.id !== 'file.recent'
      ) {
        expect(typeof c.run, c.id).toBe('function');
      }
      for (const k of c.children ?? []) {
        expect(typeof k.run, `${c.id}>${k.id}`).toBe('function');
      }
    }
    // ...and every gated row enables in SOME context (no always-disabled).
    const gated: [MenuContext, string][] = [
      [{ ...baseCtx, compiling: true }, 'tools.cancel'],
      [{ ...baseCtx, reloadPending: true }, 'file.reload'],
      [{ ...baseCtx, reloadPending: true }, 'file.keep-mine'],
      [{ ...baseCtx, recentProjects: ['/a/paper'] }, 'file.recent'],
    ];
    for (const [ctx, id] of gated) {
      const found = buildMenus(ctx, actions)
        .flatMap((s) => s.commands)
        .find((c) => c.id === id)!;
      expect(found.enabled, id).toBe(true);
      const runs = (found.children ?? [found]).map((k) => typeof k.run);
      expect(
        runs.every((t) => t === 'function'),
        id,
      ).toBe(true);
    }
  });
  it('no git-named command or label survives', () => {
    const cmds = buildMenus(baseCtx, actions).flatMap((s) => s.commands);
    const rows = cmds.flatMap((c) => [c, ...(c.children ?? [])]);
    for (const c of rows) {
      expect(c.id, c.id).not.toMatch(/git/i);
      expect(c.label, c.label).not.toMatch(/git|HEAD|commit|branch|stage|diff|repo/i);
    }
    // The history surface says it in user words and nothing else.
    const hist = cmds.find((c) => c.id === 'history.show')!;
    expect(hist.label).toBe('File History…');
  });

  it('history row is gated on availability and carries a keymapped chord', () => {
    const find = (ctx: MenuContext) =>
      buildMenus(ctx, actions)
        .flatMap((s) => s.commands)
        .find((c) => c.id === 'history.show')!;
    expect(find(baseCtx).enabled).toBe(true);
    expect(typeof find(baseCtx).run).toBe('function');
    // Nothing to list without a project file or a reachable store.
    expect(find({ ...baseCtx, historyAvailable: false }).enabled).toBe(false);
    // Chord parity: the accelerator is a KEYMAP row and the chord resolves.
    expect(find(baseCtx).accelerator).toBe('Ctrl+H');
    expect(KEYMAP.some((k) => k.keys === 'Ctrl+H')).toBe(true);
    const ev = { ctrlKey: true, metaKey: false, shiftKey: false, altKey: false, key: 'h' };
    expect(menuChordId(ev as unknown as KeyboardEvent)).toBe('history.show');
  });
  it('recent projects submenu lists recents, disabled when empty', () => {
    const all = buildMenus(baseCtx, actions).flatMap((s) => s.commands);
    expect(all.find((c) => c.id === 'file.recent')!.enabled).toBe(false);
    const withRecents = buildMenus(
      { ...baseCtx, recentProjects: ['/b/thesis', '/a/paper'] },
      actions,
    )
      .flatMap((s) => s.commands)
      .find((c) => c.id === 'file.recent')!;
    expect(withRecents.enabled).toBe(true);
    expect(withRecents.children?.map((k) => k.label)).toEqual(['thesis', 'paper']);
    for (const k of withRecents.children ?? []) {
      expect(typeof k.run, k.label).toBe('function');
    }
  });
  it('ids are unique across sections', () => {
    const ids = buildMenus(baseCtx, actions).flatMap((s) => s.commands.map((c) => c.id));
    expect(new Set(ids).size).toBe(ids.length);
  });
  it('dialog renders the same KEYMAP table (no second key list)', () => {
    // The dialog renders one row per KEYMAP entry, same ids, same labels —
    // this pin fails if anyone introduces a parallel key list.
    expect(KEYMAP.length).toBeGreaterThan(0);
    const ids = KEYMAP.map((k) => k.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const k of KEYMAP) {
      expect(k.label.length, k.id).toBeGreaterThan(0);
      expect(k.keys.length, k.id).toBeGreaterThan(0);
    }
    // Registry accelerators stay a SUBSET of KEYMAP keys (parity both ways:
    // no menu chord missing from the dialog, no dialog row missing a meaning).
    const keys = new Set(KEYMAP.map((k) => k.keys));
    for (const s of buildMenus(baseCtx, actions)) {
      for (const c of s.commands) {
        if (c.accelerator) expect(keys.has(c.accelerator), c.id).toBe(true);
      }
    }
  });
  it('outline submenu caps stay deliberate (25) and labeled honest', () => {
    // Submenu rows cap at PICK_CAP with the full outline one click away in
    // the tree column. 101 sections → 25 rows, no silent truncation.
    const lines = Array.from({ length: 101 }, (_, i) => ({ line: i + 1, title: `S${i + 1}` }));
    const all = buildMenus({ ...baseCtx, outlineLines: lines }, actions).flatMap((s) => s.commands);
    expect(all.find((c) => c.id === 'selection.pick-one')!.children?.length).toBe(25);
    expect(all.find((c) => c.id === 'selection.pick-many')!.children?.length).toBe(25);
    // Parse itself keeps full fidelity (DATA cap 1000, not the view cap).
    const text = Array.from({ length: 30 }, (_, i) => `\\section{S${i + 1}}`).join('\n');
    expect(parseOutline(text).length).toBe(30);
  });
  it('disabled states match context (compiling / no-project / trash-empty)', () => {
    const find = (ctx: MenuContext, id: string) =>
      buildMenus(ctx, actions)
        .flatMap((s) => s.commands)
        .find((c) => c.id === id)!;
    expect(find({ ...baseCtx, compiling: true }, 'tools.compile').enabled).toBe(false);
    expect(find({ ...baseCtx, compiling: true }, 'tools.cancel').enabled).toBe(true);
    expect(find({ ...baseCtx, hasProject: false }, 'file.new-file').enabled).toBe(false);
    expect(find({ ...baseCtx, canUndoDelete: false }, 'edit.undo-delete').enabled).toBe(false);
    expect(find({ ...baseCtx, reloadPending: true }, 'file.reload').visible).toBe(true);
    expect(find(baseCtx, 'file.reload').visible).toBe(false);
  });
});
