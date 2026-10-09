// presence.ts — what this window has open (project, main file, file in
// front), published for the MCP server's read view of the running app, and
// an agent's ask that the window open a project, which only the user's
// click answers. The Rust side keeps the presence alive on a heartbeat and
// removes it on exit.

import { request } from './core-request.tauri';
import type { OpenOutcome, OpenRequest } from './generated/api';

export type { OpenOutcome, OpenRequest };

/** Publish; failures are silent (the agent's view is a convenience). */
export function publishPresence(
  rootId: string | null,
  mainRel: string | null,
  activeRel: string | null,
): void {
  void request('presenceSet', { rootId, mainRel, activeRel }).catch(() => {});
}

/** An agent's ask that this window open a project, while unanswered. */
export function pendingOpenRequest(): Promise<OpenRequest | null> {
  return request('presencePending', {});
}

/** The user's answer to the ask `id`. */
export function answerOpenRequest(id: string, outcome: OpenOutcome): Promise<void> {
  return request('presenceAnswer', { id, outcome }).then(() => undefined);
}
