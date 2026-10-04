import { describe, expect, it } from 'vitest';
import { cancelRun, canCancel, isCancelled, startRun, statusText } from './exportProgress';

describe('exportProgress', () => {
  it('a run can be cancelled once', () => {
    const run = startRun('single-file bundle');
    expect(canCancel(run)).toBe(true);
    const after = cancelRun(run);
    expect(canCancel(after)).toBe(false);
    expect(after?.cancelling).toBe(true);
    expect(run.cancelling).toBe(false);
  });

  it('no run, nothing to cancel', () => {
    expect(canCancel(null)).toBe(false);
    expect(cancelRun(null)).toBeNull();
  });

  it('the status names the run and the cancelling state', () => {
    const run = startRun('browser preview');
    expect(statusText(run)).toBe('Exporting the browser preview...');
    expect(statusText(cancelRun(run)!)).toBe('Cancelling the browser preview export...');
  });

  it('tells a cancelled export from a failed one', () => {
    expect(isCancelled('export cancelled')).toBe(true);
    expect(isCancelled(new Error('the conversion was canceled'))).toBe(true);
    expect(isCancelled('disk full')).toBe(false);
    expect(isCancelled('uncancelled')).toBe(false);
  });
});
