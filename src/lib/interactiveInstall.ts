// interactiveInstall.ts — put maleficium-interactive.sty into the open project.
//
// The package ships inside the app and reaches a project only by this
// explicit action (command palette, or the fix on a missing-package warning
// or compile error). The backend refuses to replace a modified copy until
// the user confirms; the MCP tool has no such path.

import { request } from './core-request.tauri';
import type { InstallResult } from './generated/api';
import type { InstallOutcome } from './generated/events';
import type { Finding, MissingDependency } from './generated/structure';

/** The package file name, as the pre-compile check and the engine name it. */
export const INTERACTIVE_PACKAGE = 'maleficium-interactive.sty';

/** A pre-compile finding for exactly the interactive package. */
export function findingNeedsInstall(f: Finding): boolean {
  return f.kind === 'not-in-bundle' && f.name === INTERACTIVE_PACKAGE;
}

/** A compile's missing dependency that is exactly the interactive package. */
export function missingNeedsInstall(m: MissingDependency | null): boolean {
  return m != null && m.reason === 'not-in-bundle' && m.file === INTERACTIVE_PACKAGE;
}

/**
 * Install into the session root. Without `overwrite` a modified copy stays
 * untouched and the outcome is `needs-confirmation`.
 */
export async function installInteractive(
  rootId: string,
  overwrite: boolean,
): Promise<InstallResult> {
  return request('interactiveInstall', { rootId, overwrite });
}

/** The log line for an install outcome. */
export function describeInstall(o: InstallOutcome): string {
  switch (o) {
    case 'installed':
      return `installed ${INTERACTIVE_PACKAGE} into the project`;
    case 'already-current':
      return `${INTERACTIVE_PACKAGE} is already in the project and up to date`;
    case 'needs-confirmation':
      return `${INTERACTIVE_PACKAGE} in the project was modified: confirm to replace it`;
  }
}
