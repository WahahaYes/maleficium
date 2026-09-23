import { describe, it, expect, vi } from 'vitest';
import { highlightRuns, paletteCommands, parsePaletteInput } from './palette';
import type { MenuSection } from './commands';

const sections = (save: () => void): MenuSection[] => [
  {
    id: 'file',
    title: 'File',
    commands: [
      { id: 'file.save', label: 'Save', accelerator: 'Ctrl+S', enabled: true, run: save },
      { id: 'file.clean', label: 'Clean', enabled: false, run: () => {} },
      { id: 'file.reload', label: 'Reload', enabled: true, visible: false, run: () => {} },
      {
        id: 'file.recent',
        label: 'Open Recent',
        enabled: true,
        children: [{ id: 'file.recent', label: 'paper', enabled: true, run: () => {} }],
      },
    ],
  },
  {
    id: 'view',
    title: 'View',
    commands: [
      { id: 'view.command-palette', label: 'Command Palette…', enabled: true, run: () => {} },
    ],
  },
];

describe('palette commands', () => {
  it('lists enabled visible leaves with their menu path and runs them', () => {
    const save = vi.fn();
    const cmds = paletteCommands(sections(save), ['view.command-palette']);
    expect(cmds.map((c) => c.label)).toEqual(['File › Save', 'File › Open Recent › paper']);
    expect(cmds[0].accelerator).toBe('Ctrl+S');
    void cmds[0].run();
    expect(save).toHaveBeenCalledOnce();
  });

  it('switches to commands on a leading >', () => {
    expect(parsePaletteInput('>  save')).toEqual({ mode: 'commands', query: 'save' });
    expect(parsePaletteInput('main')).toEqual({ mode: 'files', query: 'main' });
  });

  it('splits labels into highlighted runs', () => {
    expect(highlightRuns('main.tex', [0, 1, 5])).toEqual([
      { text: 'ma', hit: true },
      { text: 'in.', hit: false },
      { text: 't', hit: true },
      { text: 'ex', hit: false },
    ]);
  });
});
