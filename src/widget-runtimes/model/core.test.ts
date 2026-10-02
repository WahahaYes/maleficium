import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { checkGlb, clip, frame, parseBackground, parseCamera, parseSize } from './core';

const fixture = () => {
  const b = readFileSync('e2e/fixtures/interactive/models/mesh.glb');
  return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) as ArrayBuffer;
};

function glbWith(doc: object): ArrayBuffer {
  const json = new TextEncoder().encode(JSON.stringify(doc));
  const pad = (4 - (json.length % 4)) % 4;
  const out = new Uint8Array(20 + json.length + pad);
  const v = new DataView(out.buffer);
  v.setUint32(0, 0x46546c67, true);
  v.setUint32(4, 2, true);
  v.setUint32(8, out.length, true);
  v.setUint32(12, json.length + pad, true);
  v.setUint32(16, 0x4e4f534a, true);
  out.set(json, 20);
  out.fill(0x20, 20 + json.length);
  return out.buffer;
}

describe('checkGlb', () => {
  it('accepts the generated fixture', () => {
    expect(checkGlb(fixture())).toBeNull();
  });
  it('refuses the old stub text, a truncated file and a wrong version', () => {
    expect(checkGlb(new TextEncoder().encode('GLB-STUB-NOT-A-MODEL\n').buffer)).toMatch(
      /not a binary glTF/,
    );
    expect(checkGlb(fixture().slice(0, 100))).toMatch(/truncated/);
    const v1 = fixture();
    new DataView(v1).setUint32(4, 1, true);
    expect(checkGlb(v1)).toMatch(/2\.0/);
    expect(checkGlb(new ArrayBuffer(4))).toMatch(/too small/);
  });
  it('refuses a model that requires a decoder the sandbox lacks', () => {
    expect(checkGlb(glbWith({ asset: { version: '2.0' } }))).toBeNull();
    expect(
      checkGlb(
        glbWith({ asset: { version: '2.0' }, extensionsRequired: ['KHR_draco_mesh_compression'] }),
      ),
    ).toMatch(/KHR_draco_mesh_compression/);
    expect(
      checkGlb(glbWith({ asset: { version: '2.0' }, extensionsRequired: ['KHR_materials_unlit'] })),
    ).toBeNull();
  });
});

describe('frame', () => {
  it('puts the camera far enough to see the sphere, farther in a narrow view', () => {
    const wide = frame(1, 45, 2);
    const narrow = frame(1, 45, 0.5);
    expect(wide.distance).toBeGreaterThan(1);
    expect(narrow.distance).toBeGreaterThan(wide.distance);
    expect(wide.near).toBeLessThan(wide.distance);
    expect(wide.far).toBeGreaterThan(wide.distance);
  });
  it('survives a degenerate radius', () => {
    expect(frame(0, 45, 1).distance).toBeGreaterThan(0);
    expect(Number.isFinite(frame(NaN, 45, 1).distance)).toBe(true);
  });
});

describe('poster options', () => {
  const id = '1 0 0 0 0 1 0 0 0 0 1 0 0 0 5 1';
  it('reads a canonical camera matrix, with commas or as an array', () => {
    expect(parseCamera(id)).toEqual([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 5, 1]);
    expect(parseCamera(id.split(' ').join(','))).toEqual(parseCamera(id));
    expect(parseCamera([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 5, 1])).toEqual(parseCamera(id));
  });
  it('ignores a malformed or degenerate camera', () => {
    expect(parseCamera(undefined)).toBeNull();
    expect(parseCamera('1 0 0')).toBeNull();
    expect(parseCamera(id.replace('5', 'x'))).toBeNull();
    expect(parseCamera('1 0 0 0 0 0 0 0 0 0 1 0 0 0 5 1')).toBeNull();
    expect(parseCamera('pos=0,0,5 target=0,0,0')).toBeNull();
  });
  it('reads size and background', () => {
    expect(parseSize('320x240')).toEqual({ width: 320, height: 240 });
    for (const bad of ['320', '8x8', '5000x10', '320X240', 320, null]) {
      expect(parseSize(bad)).toBeNull();
    }
    expect(parseBackground('#FFFFFF')).toBe('#ffffff');
    expect(parseBackground('transparent')).toBe('transparent');
    expect(parseBackground('ffffff')).toBeNull();
    expect(parseBackground('red')).toBeNull();
  });
  it('keeps the sphere between the clip planes, also from inside it', () => {
    const outside = clip(10, 1);
    expect(outside.near).toBeLessThan(9);
    expect(outside.far).toBeGreaterThan(11);
    const inside = clip(0.2, 1);
    expect(inside.near).toBeGreaterThan(0);
    expect(inside.near).toBeLessThan(0.01);
    expect(inside.far).toBeGreaterThan(1.2);
  });
});
