// precheckPanel.ts — when the pre-compile panel opens on its own.
//
// It pops after a user compile whose findings differ from the set last shown
// for that target, never for auto-compile or warm-open, and never once the
// user has turned the popup off. The status-bar chip opens it regardless.

import type { Finding } from './generated/structure';
import { DEVICE_PREF_KEYS, store } from './app-store';

/** Order-free identity of a finding set; empty for no findings. */
export function findingsKey(findings: readonly Finding[]): string {
  return findings
    .map((f) => `${f.kind}|${f.name}|${f.path}|${f.line}`)
    .sort()
    .join('\n');
}

export interface PopInput {
  /** The popup setting. */
  enabled: boolean;
  /** Whether the run may pop the panel (a user compile, not auto or warm). */
  popup: boolean;
  key: string;
  /** Key last shown for this target, if any. */
  lastShown: string | undefined;
}

export function shouldPop(i: PopInput): boolean {
  return i.enabled && i.popup && i.key !== '' && i.key !== i.lastShown;
}

/** The popup setting; on unless the user turned it off. */
export function parsePrecheckPopup(raw: string | null): boolean {
  return raw !== 'false';
}

export function loadPrecheckPopup(): boolean {
  try {
    return parsePrecheckPopup(store().get(DEVICE_PREF_KEYS.precheckPopup));
  } catch {
    return true;
  }
}

export function savePrecheckPopup(on: boolean): void {
  try {
    store().set(DEVICE_PREF_KEYS.precheckPopup, String(on));
  } catch {
    // No app store configured: the setting lasts for this session only.
  }
}
