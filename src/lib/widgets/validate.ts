// validate.ts — bridge protocol 1, runtime to host, for hosts that run in a
// browser (the reader). The rules and the `reason` names are the spec in
// src-tauri/widget-host/vectors/bridge.json, which this and the app's Rust
// validator are both pinned against.

export const LIMITS = {
  message: 200,
  detailJson: 2048,
  heightMin: 1,
  heightMax: 20000,
  requestId: 64,
  png: 8 * 1024 * 1024,
} as const;

const PNG_PREFIX = 'data:image/png;base64,';

export type BridgeMessage =
  | { type: 'ready' }
  | {
      type: 'status';
      state: 'loading' | 'loaded' | 'error';
      message?: string;
      detail?: Record<string, unknown>;
    }
  | { type: 'resize'; height: number }
  | { type: 'snapshot'; requestId: string; png: string };

export type Refusal =
  | 'not-object'
  | 'no-mfw'
  | 'type'
  | 'fields'
  | 'state'
  | 'message'
  | 'detail'
  | 'height'
  | 'request-id'
  | 'png';

export type Verdict = { ok: true; message: BridgeMessage } | { ok: false; reason: Refusal };

const isObject = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

const refuse = (reason: Refusal): Verdict => ({ ok: false, reason });

function only(m: Record<string, unknown>, allowed: string[]): boolean {
  return Object.keys(m).every((k) => k === 'mfw' || k === 'type' || allowed.includes(k));
}

/** Accepts a runtime message or names the rule it broke. */
export function validate(v: unknown): Verdict {
  if (!isObject(v)) return refuse('not-object');
  if (v.mfw !== 1) return refuse('no-mfw');
  switch (v.type) {
    case 'ready':
      return only(v, []) ? { ok: true, message: { type: 'ready' } } : refuse('fields');
    case 'status': {
      const state = v.state;
      if (state !== 'loading' && state !== 'loaded' && state !== 'error') return refuse('state');
      const out: BridgeMessage = { type: 'status', state };
      if ('message' in v) {
        if (typeof v.message !== 'string' || [...v.message].length > LIMITS.message) {
          return refuse('message');
        }
        out.message = v.message;
      }
      if ('detail' in v) {
        if (!isObject(v.detail) || JSON.stringify(v.detail).length > LIMITS.detailJson) {
          return refuse('detail');
        }
        out.detail = v.detail;
      }
      return only(v, ['state', 'message', 'detail'])
        ? { ok: true, message: out }
        : refuse('fields');
    }
    case 'resize': {
      const h = v.height;
      if (
        typeof h !== 'number' ||
        !Number.isInteger(h) ||
        h < LIMITS.heightMin ||
        h > LIMITS.heightMax
      ) {
        return refuse('height');
      }
      return only(v, ['height'])
        ? { ok: true, message: { type: 'resize', height: h } }
        : refuse('fields');
    }
    case 'snapshot': {
      const r = v.requestId;
      if (typeof r !== 'string' || r.length === 0 || r.length > LIMITS.requestId) {
        return refuse('request-id');
      }
      const png = v.png;
      if (typeof png !== 'string' || !png.startsWith(PNG_PREFIX) || png.length > LIMITS.png) {
        return refuse('png');
      }
      return only(v, ['requestId', 'png'])
        ? { ok: true, message: { type: 'snapshot', requestId: r, png } }
        : refuse('fields');
    }
    default:
      return refuse('type');
  }
}
