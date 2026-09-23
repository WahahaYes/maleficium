// palette.ts — what the command palette and file finder list. Pure: the
// palette reads the one command registry (menus), never a second list.

import type { CommandId, MenuCommand, MenuSection } from './commands';

/** One runnable command as the palette shows it. */
export interface PaletteCommand {
  id: CommandId;
  /** `Section › Parent › Label`, the path to it in the menus. */
  label: string;
  accelerator?: string;
  run: () => void | Promise<void>;
}

/**
 * Every enabled, visible, runnable command in the menus, flattened in menu
 * order with its menu path as the label. `exclude` drops ids (the palette's
 * own commands).
 */
export function paletteCommands(
  sections: MenuSection[],
  exclude: CommandId[] = [],
): PaletteCommand[] {
  const out: PaletteCommand[] = [];
  const walk = (cmds: MenuCommand[], path: string) => {
    for (const c of cmds) {
      if (!c.enabled || c.visible === false || exclude.includes(c.id)) continue;
      const label = `${path} › ${c.label}`;
      if (c.children) walk(c.children, label);
      else if (c.run) out.push({ id: c.id, label, accelerator: c.accelerator, run: c.run });
    }
  };
  for (const s of sections) walk(s.commands, s.title);
  return out;
}

export type PaletteMode = 'files' | 'commands';

/** `>` switches the finder to commands, as in most editors. */
export function parsePaletteInput(text: string): { mode: PaletteMode; query: string } {
  return text.startsWith('>')
    ? { mode: 'commands', query: text.slice(1).trim() }
    : { mode: 'files', query: text.trim() };
}

/** A label split at highlighted UTF-16 positions, for rendering. */
export function highlightRuns(text: string, positions: number[]): { text: string; hit: boolean }[] {
  const hits = new Set(positions);
  const runs: { text: string; hit: boolean }[] = [];
  for (let i = 0; i < text.length; i++) {
    const hit = hits.has(i);
    const last = runs[runs.length - 1];
    if (last && last.hit === hit) last.text += text[i];
    else runs.push({ text: text[i], hit });
  }
  return runs;
}
