import { describe, expect, it } from 'vitest';
import { mediaErrorMessage, videoMime } from './core';

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
