// useInteractiveInstall — the app's install of maleficium-interactive.sty.
// A modified copy in the project is replaced only after the user confirms.

import { useState } from 'react';
import { emit } from '../lib/events';
import {
  describeInstall,
  installInteractive,
  INTERACTIVE_PACKAGE,
} from '../lib/interactiveInstall';

export function useInteractiveInstall(deps: {
  projectId: string | null;
  /** Called once the package is in the project (installed or already current). */
  onInstalled: () => void;
}) {
  const { projectId, onInstalled } = deps;
  const [confirmOpen, setConfirmOpen] = useState(false);

  async function run(overwrite: boolean) {
    if (!projectId) return;
    try {
      const r = await installInteractive(projectId, overwrite);
      emit({
        scope: 'app',
        kind: r.outcome === 'installed' ? 'success' : 'info',
        actor: 'user',
        message: describeInstall(r.outcome),
        event: { action: 'interactive.install', outcome: r.outcome },
      });
      if (r.outcome === 'needs-confirmation') setConfirmOpen(true);
      else onInstalled();
    } catch (e) {
      emit({
        scope: 'app',
        kind: 'error',
        actor: 'user',
        message: `install ${INTERACTIVE_PACKAGE} failed: ` + String(e).slice(0, 200),
        event: { action: 'interactive.install-failed', error: String(e).slice(0, 200) },
      });
    }
  }

  return {
    install: () => void run(false),
    confirmOpen,
    confirmReplace: () => {
      setConfirmOpen(false);
      void run(true);
    },
    cancelReplace: () => setConfirmOpen(false),
  };
}
