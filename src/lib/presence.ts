// presence.ts — what this window has open (project, main file, file in
// front), published for the MCP server's read view of the running app. The
// Rust side keeps it alive on a heartbeat and removes it on exit.

import { request } from './core-request.tauri';

/** Publish; failures are silent (the agent's view is a convenience). */
export function publishPresence(
  rootId: string | null,
  mainRel: string | null,
  activeRel: string | null,
): void {
  void request('presenceSet', { rootId, mainRel, activeRel }).catch(() => {});
}
