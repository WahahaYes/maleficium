import { describe, it, expect } from 'vitest';
import { hashRoot, joinPath, appTrashDir, appOutDir } from './paths';

describe('app-local paths', () => {
  it('hashRoot is deterministic 8-hex', () => {
    expect(hashRoot('/home/u/paper')).toBe(hashRoot('/home/u/paper'));
    expect(hashRoot('/home/u/paper')).toMatch(/^[0-9a-f]{8}$/);
    expect(hashRoot('/home/u/other')).not.toBe(hashRoot('/home/u/paper'));
  });
  it('trash + out dirs shard per root under app homes', () => {
    const t = appTrashDir('/app/data', '/home/u/paper');
    const o = appOutDir('/tmp', '/home/u/paper');
    expect(t.startsWith('/app/data/maleficium-trash/')).toBe(true);
    expect(o.startsWith('/tmp/maleficium-out/')).toBe(true);
    // Same root → same shard; different roots → different shards.
    expect(appTrashDir('/app/data', '/home/u/other')).not.toBe(t);
    expect(appOutDir('/tmp', '/home/u/other')).not.toBe(o);
    // Never inside the project dir.
    expect(t.startsWith('/home/u/paper')).toBe(false);
    expect(o.startsWith('/home/u/paper')).toBe(false);
  });
  it('joinPath collapses duplicate slashes', () => {
    expect(joinPath('/a/', '/b', 'c')).toBe('/a/b/c');
  });
});
