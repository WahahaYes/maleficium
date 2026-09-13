import { describe, it, expect } from 'vitest';
import { parseLog } from '../lib/parseLog';

// Re-export compat: Problems.tsx forwards lib/parseLog (LogStream consumes data refs).
describe('problems compat', () => {
  it('re-export parses clickable entries for the stream', async () => {
    const { parseLog: viaCompat } = await import('./Problems');
    expect(viaCompat('error: hello.tex:3: boom', '/tmp/x', '/tmp/x')).toEqual(
      parseLog('error: hello.tex:3: boom', '/tmp/x', '/tmp/x'),
    );
  });
});
