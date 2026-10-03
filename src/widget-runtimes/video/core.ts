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
