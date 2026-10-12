import { Annotation, type Transaction } from '@codemirror/state';

/**
 * Marks a transaction that loads text the app already holds (a file switch, a
 * reload, a jump). The editor must not report it back as an edit: the echo
 * reached the buffer map before the freshly opened file's buffer existed and
 * marked it dirty, so every open autosaved and recompiled.
 */
export const externalSync = Annotation.define<boolean>();

/** Whether any of these transactions is an edit the user (or a command) made. */
export function hasUserEdit(transactions: readonly Transaction[]): boolean {
  return transactions.some((tr) => tr.docChanged && !tr.annotation(externalSync));
}
