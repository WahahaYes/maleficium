import { describe, expect, it } from 'vitest';
import { flag } from './options';

describe('flag', () => {
  it('reads booleans, numbers and words', () => {
    expect([true, 1, 'true', ' Yes ', 'on', '1'].map(flag)).toEqual(Array(6).fill(true));
    expect([false, 0, 'false', 'no', '', undefined, null, {}].map(flag)).toEqual(
      Array(8).fill(false),
    );
  });
});
