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
