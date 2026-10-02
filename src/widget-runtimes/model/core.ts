// Pure helpers of the model runtime (`model@1`): checks on the glb bytes before
// they reach the loader, and the camera framing. No three.js here, so they test
// without a renderer.

const GLB_MAGIC = 0x46546c67; // "glTF"
const JSON_CHUNK = 0x4e4f534a; // "JSON"

// Extensions the offline viewer cannot decode: each needs a wasm or worker
// decoder the sandbox does not provide.
const UNSUPPORTED = [
  'KHR_draco_mesh_compression',
  'EXT_meshopt_compression',
  'KHR_meshopt_compression',
  'KHR_texture_basisu',
];

/** Why these bytes cannot be shown, or null when they look like a viewable glb. */
export function checkGlb(bytes: ArrayBuffer): string | null {
  if (bytes.byteLength < 20) return 'the model file is too small to be a glb';
  const v = new DataView(bytes);
  if (v.getUint32(0, true) !== GLB_MAGIC) {
    return 'the model is not a binary glTF (.glb) file';
  }
  if (v.getUint32(4, true) !== 2) return 'only glTF 2.0 models are supported';
  const total = v.getUint32(8, true);
  if (total > bytes.byteLength) return 'the model file is truncated';
  const jsonLen = v.getUint32(12, true);
  if (v.getUint32(16, true) !== JSON_CHUNK || 20 + jsonLen > bytes.byteLength) {
    return 'the model file has no readable glTF header';
  }
  let doc: unknown;
  try {
    doc = JSON.parse(new TextDecoder().decode(new Uint8Array(bytes, 20, jsonLen)));
  } catch {
    return 'the model file has no readable glTF header';
  }
  const required = (doc as { extensionsRequired?: unknown }).extensionsRequired;
  if (Array.isArray(required)) {
    const bad = required.filter(
      (e): e is string => typeof e === 'string' && UNSUPPORTED.includes(e),
    );
    if (bad.length) return `the model needs ${bad[0]}, which the offline viewer cannot decode`;
  }
  return null;
}

export interface Framing {
  distance: number;
  near: number;
  far: number;
}

/** Camera distance that fits a bounding sphere of `radius` in a view of `fovDeg`, and clip planes. */
export function frame(radius: number, fovDeg: number, aspect: number): Framing {
  const r = radius > 0 && Number.isFinite(radius) ? radius : 1;
  const half = (fovDeg * Math.PI) / 360;
  const hfov = Math.atan(Math.tan(half) * Math.max(aspect, 0.01));
  const distance = (r / Math.sin(Math.min(half, hfov))) * 1.1;
  return { distance, near: distance / 100, far: distance * 100 };
}

/**
 * The `camera` option: a camera-to-world matrix, 16 numbers in column-major
 * order (glTF's and three.js's), as the app canonicalizes it (space
 * separated). Null when absent or malformed: the view then frames the model.
 */
export function parseCamera(v: unknown): number[] | null {
  const parts = Array.isArray(v)
    ? v
    : typeof v === 'string'
      ? v.split(/[\s,]+/).filter((t) => t !== '')
      : [];
  const m = parts.map((t) => (typeof t === 'number' ? t : Number(t)));
  if (m.length !== 16 || !m.every(Number.isFinite)) return null;
  // The forward (-z) and up columns must point somewhere.
  const len = (i: number) => Math.hypot(m[i], m[i + 1], m[i + 2]);
  return len(4) > 1e-9 && len(8) > 1e-9 ? m : null;
}

export interface Size {
  width: number;
  height: number;
}

/** The `size` option, `WxH` in pixels (16 to 4096 a side); null otherwise. */
export function parseSize(v: unknown): Size | null {
  const m = typeof v === 'string' ? /^(\d{1,4})x(\d{1,4})$/.exec(v.trim()) : null;
  if (!m) return null;
  const [width, height] = [Number(m[1]), Number(m[2])];
  const ok = (n: number) => n >= 16 && n <= 4096;
  return ok(width) && ok(height) ? { width, height } : null;
}

/** The `background` option: `transparent` or `#rrggbb`; null otherwise. */
export function parseBackground(v: unknown): string | null {
  if (typeof v !== 'string') return null;
  const t = v.trim().toLowerCase();
  return t === 'transparent' || /^#[0-9a-f]{6}$/.test(t) ? t : null;
}

/** Clip planes for a camera `distance` from the centre of a bounding sphere of `radius`. */
export function clip(distance: number, radius: number): { near: number; far: number } {
  const r = radius > 0 && Number.isFinite(radius) ? radius : 1;
  const d = Number.isFinite(distance) ? Math.max(distance, 0) : r * 3;
  const near = Math.max(d - r * 1.5, r / 1000, d / 1000);
  return { near, far: d + r * 2 };
}
