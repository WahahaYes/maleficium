// exportProgress.ts — the state of a running paper export, for its Cancel button.
//
// Only the bundle export can be cancelled (it is the one that converts the
// paper); a cancelled export fails with a message naming the cancellation
// and writes nothing, so that failure is told apart from a real one.

export type ExportRun = {
  /** What is being made, e.g. `single-file bundle` or `browser preview`. */
  label: string;
  /** Cancel was asked and the export has not yet stopped. */
  cancelling: boolean;
};

export function startRun(label: string): ExportRun {
  return { label, cancelling: false };
}

/** The run after Cancel was pressed; a finished run (null) stays finished. */
export function cancelRun(run: ExportRun | null): ExportRun | null {
  return run ? { ...run, cancelling: true } : null;
}

/** Cancel can be pressed once per run. */
export function canCancel(run: ExportRun | null): boolean {
  return run != null && !run.cancelling;
}

export function statusText(run: ExportRun): string {
  return run.cancelling ? `Cancelling the ${run.label} export...` : `Exporting the ${run.label}...`;
}

/** Whether an export's failure is the user's own cancellation. */
export function isCancelled(err: unknown): boolean {
  return /\bcancel(l)?ed\b/i.test(String(err));
}
