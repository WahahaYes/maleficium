// The video runtime (`video@1`): a native <video> with controls, playing the
// widget's video from bytes delivered in `init` through a blob URL (the policy
// allows media from blob: and data: only). The host keeps the poster over this
// frame until `status loaded` arrives. Options: `loop` and `muted`.
import { applyTheme, startBridge, status, type Init } from '../bridge';
import { flag } from '../options';
import { mediaErrorMessage, videoMime } from './core';

const video = document.getElementById('v') as HTMLVideoElement;
const note = document.getElementById('msg') as HTMLElement;
let url: string | null = null;
let settled = false;

function fail(message: string): void {
  settled = true;
  note.hidden = false;
  note.textContent = message;
  status('error', message);
}

function loaded(): void {
  if (settled) return;
  settled = true;
  note.hidden = true;
  status('loaded');
}

video.addEventListener('loadeddata', loaded);
video.addEventListener('error', () => fail(mediaErrorMessage(video.error?.code)));

function init(msg: Init): void {
  applyTheme(msg.theme);
  video.setAttribute('aria-label', msg.alt);
  settled = false;
  status('loading');
  const src = msg.sources.video;
  if (!src) return fail('the video widget has no "video" source');
  if (url) URL.revokeObjectURL(url);
  video.loop = flag(msg.options.loop);
  video.muted = flag(msg.options.muted);
  url = URL.createObjectURL(new Blob([src.bytes], { type: videoMime(src.mime, src.name) }));
  video.src = url;
  video.load();
}

/** The frame on screen over the figure background, as a PNG data URL. */
function snapshot(): string | null {
  if (!video.videoWidth || video.readyState < 2) return null;
  const c = document.createElement('canvas');
  c.width = video.videoWidth;
  c.height = video.videoHeight;
  const ctx = c.getContext('2d');
  if (!ctx) return null;
  ctx.fillStyle = getComputedStyle(document.body).backgroundColor;
  ctx.fillRect(0, 0, c.width, c.height);
  ctx.drawImage(video, 0, 0);
  try {
    return c.toDataURL('image/png');
  } catch {
    return null;
  }
}

startBridge({ onInit: init, onTheme: applyTheme, onSnapshot: snapshot });
