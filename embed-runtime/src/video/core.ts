// Pure helpers of the video runtime (`video@1`).

const BY_EXTENSION: Record<string, string> = {
  mp4: 'video/mp4',
  m4v: 'video/mp4',
  webm: 'video/webm',
  ogv: 'video/ogg',
  ogg: 'video/ogg',
  mov: 'video/quicktime',
};

/** The media type to give the blob: the host's when it is a video type, else the file name's. */
export function videoMime(mime: string, name: string): string {
  if (/^video\/[a-z0-9.+-]+$/i.test(mime)) return mime;
  const ext = name.slice(name.lastIndexOf('.') + 1).toLowerCase();
  return BY_EXTENSION[ext] ?? 'video/mp4';
}

/** A readable reason for a MediaError code. */
export function mediaErrorMessage(code: number | undefined): string {
  switch (code) {
    case 3:
      return 'the video is damaged or cannot be decoded';
    case 4:
      return 'this video format is not supported here';
    default:
      return 'the video failed to load';
  }
}

/**
 * The moment `at=` asks for, in seconds, kept inside the media: from 0 to just
 * before its end (the last frame). Null when the option is absent or not a
 * time.
 */
export function seekTarget(at: unknown, duration: number): number | null {
  const t = typeof at === 'number' ? at : typeof at === 'string' && at.trim() ? Number(at) : NaN;
  if (!Number.isFinite(t) || t < 0) return null;
  if (!Number.isFinite(duration) || duration <= 0) return t;
  return Math.min(t, Math.max(0, duration - 0.001));
}

/**
 * Whether a drawn frame is blank: every sampled pixel transparent, or every
 * one the colour of the first (the fill under the frame, nothing drawn on
 * it). `rgba` is canvas image data.
 */
export function isBlankFrame(rgba: ArrayLike<number>): boolean {
  if (rgba.length < 4) return true;
  let clear = true;
  let flat = true;
  for (let i = 0; i + 3 < rgba.length; i += 4) {
    if (rgba[i + 3] !== 0) clear = false;
    if (
      rgba[i] !== rgba[0] ||
      rgba[i + 1] !== rgba[1] ||
      rgba[i + 2] !== rgba[2] ||
      rgba[i + 3] !== rgba[3]
    )
      flat = false;
    if (!clear && !flat) return false;
  }
  return true;
}
