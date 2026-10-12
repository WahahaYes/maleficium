import { useCallback, useState } from 'react';
import type { AppDialog } from '../lib/commandActions';
import type { TemplateDialogMode } from '../components/TemplateDialogs';

// What the menus open: one modal at a time, the file finder / command
// palette (by prefix: `>` lists commands), and the template dialogs.
export function useDialogs() {
  const [current, show] = useState<AppDialog | null>(null);
  const [palettePrefix, palette] = useState<string | null>(null);
  const [templateMode, template] = useState<TemplateDialogMode>(null);
  const close = useCallback(() => show(null), []);
  return { current, show, close, palettePrefix, palette, templateMode, template };
}

export type Dialogs = ReturnType<typeof useDialogs>;
