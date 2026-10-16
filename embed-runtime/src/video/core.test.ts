import { describe, expect, it } from 'vitest';
import { isBlankFrame, mediaErrorMessage, seekTarget, videoMime } from './core';

describe('videoMime', () => {
  it('keeps a video type and otherwise reads the extension', () => {
    expect(videoMime('video/webm', 'x.bin')).toBe('video/webm');
    expect(videoMime('', 'clip.MP4')).toBe('video/mp4');
    expect(videoMime('application/octet-stream', 'a.ogv')).toBe('video/ogg');
    expect(videoMime('', 'noext')).toBe('video/mp4');
  });
});

describe('mediaErrorMessage', () => {
  it('names the decode and format failures', () => {
    expect(mediaErrorMessage(3)).toMatch(/decoded/);
    expect(mediaErrorMessage(4)).toMatch(/not supported/);
    expect(mediaErrorMessage(undefined)).toMatch(/failed to load/);
  });
});

describe('seekTarget', () => {
  it('reads seconds from a number or text and keeps them inside the media', () => {
    expect(seekTarget(2.5, 10)).toBe(2.5);
    expect(seekTarget('1.5', 10)).toBe(1.5);
    expect(seekTarget(0, 10)).toBe(0);
    expect(seekTarget(12, 10)).toBeCloseTo(9.999);
    expect(seekTarget(3, NaN)).toBe(3);
  });
  it('is null without a usable time', () => {
    for (const v of [undefined, null, '', 'soon', -1, Infinity, true]) {
      expect(seekTarget(v, 10)).toBeNull();
    }
  });
});

describe('isBlankFrame', () => {
  it('finds transparent and single-colour frames', () => {
    expect(isBlankFrame(new Uint8ClampedArray(16))).toBe(true);
    expect(isBlankFrame([255, 255, 255, 255, 255, 255, 255, 255])).toBe(true);
    expect(isBlankFrame([255, 255, 255, 255, 10, 20, 30, 255])).toBe(false);
    expect(isBlankFrame([0, 0, 0, 0, 0, 0, 0, 255])).toBe(false);
    expect(isBlankFrame([])).toBe(true);
  });
});
