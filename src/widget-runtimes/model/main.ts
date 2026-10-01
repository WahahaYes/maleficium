// The model runtime (`model@1`): shows the widget's glb with orbit controls,
// from bytes delivered in `init`. See ./core.ts. The host keeps the poster over
// this frame until `status loaded` arrives.
import {
  Box3,
  Color,
  DirectionalLight,
  HemisphereLight,
  Mesh,
  Object3D,
  PerspectiveCamera,
  Scene,
  Sphere,
  Vector3,
  WebGLRenderer,
  type Material,
} from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { applyTheme, startBridge, status, type Init } from '../bridge';
import { checkGlb, frame } from './core';

const FOV = 40;
const host = document.getElementById('view') as HTMLElement;
const note = document.getElementById('msg') as HTMLElement;

let renderer: WebGLRenderer | null = null;
let controls: OrbitControls | null = null;
let model: Object3D | null = null;
const scene = new Scene();
const camera = new PerspectiveCamera(FOV, 1, 0.1, 1000);
// Lights ride with the camera, so a model never goes dark when it is orbited.
camera.add(new DirectionalLight(0xffffff, 2.2));
scene.add(camera, new HemisphereLight(0xffffff, 0x8a8a95, 1.6));

function fail(message: string): void {
  note.hidden = false;
  note.textContent = message;
  status('error', message);
}

function background(): string {
  return getComputedStyle(document.body).backgroundColor;
}

function render(): void {
  renderer?.render(scene, camera);
}

function resize(): void {
  if (!renderer) return;
  const w = Math.max(host.clientWidth, 1);
  const h = Math.max(host.clientHeight, 1);
  renderer.setSize(w, h, false);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
  render();
}

function ensureRenderer(): WebGLRenderer {
  if (renderer) return renderer;
  const r = new WebGLRenderer({ antialias: true, alpha: true });
  r.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  r.setClearColor(0x000000, 0);
  r.domElement.tabIndex = 0;
  host.prepend(r.domElement);
  const c = new OrbitControls(camera, r.domElement);
  c.enableDamping = false;
  c.listenToKeyEvents(r.domElement);
  c.addEventListener('change', render);
  renderer = r;
  controls = c;
  return r;
}

function dispose(o: Object3D): void {
  o.traverse((n) => {
    const m = n as Mesh;
    m.geometry?.dispose();
    for (const mat of ([] as Material[]).concat(m.material ?? [])) mat.dispose();
  });
}

function fit(obj: Object3D): void {
  const box = new Box3().setFromObject(obj);
  const sphere = box.getBoundingSphere(new Sphere());
  const center = sphere.center;
  const f = frame(sphere.radius, FOV, camera.aspect);
  camera.near = f.near;
  camera.far = f.far;
  camera.position
    .copy(center)
    .add(new Vector3(0.6, 0.45, 1).normalize().multiplyScalar(f.distance));
  camera.updateProjectionMatrix();
  controls!.target.copy(center);
  controls!.update();
}

function parse(bytes: ArrayBuffer): Promise<Object3D> {
  return new Promise((resolve, reject) => {
    // The loader decodes embedded images through fetch unless it thinks
    // createImageBitmap is missing; the sandbox policy forbids fetch.
    const g = globalThis as { createImageBitmap?: unknown };
    const saved = g.createImageBitmap;
    g.createImageBitmap = undefined;
    try {
      new GLTFLoader().parse(
        bytes,
        '',
        (gltf) => resolve(gltf.scene),
        (e) => reject(e instanceof Error ? e : new Error('the model could not be parsed')),
      );
    } catch (e) {
      reject(e instanceof Error ? e : new Error('the model could not be parsed'));
    } finally {
      g.createImageBitmap = saved;
    }
  });
}

async function init(msg: Init): Promise<void> {
  applyTheme(msg.theme);
  host.setAttribute('role', 'img');
  host.setAttribute('aria-label', msg.alt);
  status('loading');
  try {
    const src = msg.sources.model;
    if (!src) return fail('the model widget has no "model" source');
    const bad = checkGlb(src.bytes);
    if (bad) return fail(bad);
    const loaded = await parse(src.bytes);
    const r = ensureRenderer();
    if (model) {
      scene.remove(model);
      dispose(model);
    }
    model = loaded;
    scene.add(model);
    r.domElement.setAttribute('aria-label', msg.alt);
    resize();
    fit(model);
    render();
    note.hidden = true;
    status('loaded');
  } catch (err) {
    fail(err instanceof Error ? err.message : 'the model failed to load');
  }
}

/** The current view over the figure background, as a PNG data URL. */
function snapshot(): string | null {
  if (!renderer || !model) return null;
  render();
  const src = renderer.domElement;
  const c = document.createElement('canvas');
  c.width = src.width;
  c.height = src.height;
  const ctx = c.getContext('2d');
  if (!ctx) return null;
  ctx.fillStyle = new Color(background()).getStyle();
  ctx.fillRect(0, 0, c.width, c.height);
  ctx.drawImage(src, 0, 0);
  return c.toDataURL('image/png');
}

startBridge({
  onInit: (m) => void init(m),
  onTheme: (t) => {
    applyTheme(t);
  },
  onSnapshot: snapshot,
});
window.addEventListener('resize', resize);
