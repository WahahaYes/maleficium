import { describe, expect, it } from 'vitest';
import spec from '../../../src-tauri/widget-host/vectors/bridge.json';
import { LIMITS, validate } from './validate';

describe('bridge validation against the shared spec', () => {
  it('holds every vector', () => {
    expect(spec.cases.length).toBeGreaterThanOrEqual(30);
    for (const c of spec.cases) {
      const v = validate(c.message);
      if (c.accept) expect(v, c.name).toMatchObject({ ok: true });
      else expect(v, c.name).toEqual({ ok: false, reason: (c as { reason: string }).reason });
    }
  });

  it('uses the spec limits', () => {
    expect(spec.limits).toEqual(LIMITS);
  });

  it('refuses a snapshot over the cap', () => {
    const png = 'data:image/png;base64,' + 'A'.repeat(LIMITS.png);
    expect(validate({ mfw: 1, type: 'snapshot', requestId: 'r', png })).toEqual({
      ok: false,
      reason: 'png',
    });
  });
});
