// Sandbox render check for the model and video runtimes: headless Firefox plays
// the reader. Each runtime document is the committed build with the exporter's
// widget policy as its first element, shown in `<iframe sandbox="allow-scripts"
// srcdoc>` (the reader's own frame), and fed its source as bytes over the
// bridge. Checks the load, a non-blank and theme-following picture, orbiting,
// bad input, a forged sibling message, and that nothing reaches the network.
// Controls: an unsandboxed frame must be able to read its parent, and a
// runtime that names an external url must trip the bundle grep.
//
//   node e2e/widget-runtimes/run.mjs [--shots <dir>]
//
// Needs a Firefox for playwright-core (npx playwright install firefox) and
// ffmpeg (a webm test clip, since a stock Firefox has no H.264 decoder).
import { firefox } from 'playwright-core';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const dev = resolve(here, '../..');
const shotsAt = process.argv.includes('--shots')
  ? process.argv[process.argv.indexOf('--shots') + 1]
  : null;
if (shotsAt) mkdirSync(shotsAt, { recursive: true });

let failed = 0;
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'ok' : 'FAIL'}: ${name}${ok ? '' : ` -- ${detail}`}`);
  if (!ok) failed++;
};

// The exporter's policy for a widget that declares no origins.
const fold = readFileSync(join(dev, 'src-tauri/core/src/bundle/fold.rs'), 'utf8');
const policy = /const WIDGET_POLICY: &str = "([^"]*)";/
  .exec(fold)[1]
  .replaceAll('{res}', '')
  .replace('{connect}', "'none'")
  .replace('{frame}', '');
const withPolicy = (html) =>
  html.replace('<head>', `<head><meta http-equiv="Content-Security-Policy" content="${policy}">`);
const runtime = (name) =>
  withPolicy(readFileSync(join(dev, `src-tauri/widget-runtimes/${name}/index.html`), 'utf8'));

const scratch = mkdtempSync(join(tmpdir(), 'maleficium-widget-runtimes-'));
const webm = join(scratch, 'clip.webm');
execFileSync(
  'ffmpeg',
  [
    '-v',
    'error',
    '-y',
    '-f',
    'lavfi',
    '-i',
    'testsrc2=size=160x90:rate=10:duration=2',
    '-c:v',
    'libvpx',
    '-b:v',
    '200k',
    webm,
  ],
  { timeout: 60_000 },
);
const b64 = (path) => readFileSync(path).toString('base64');
const glb = b64(join(dev, 'e2e/fixtures/interactive/models/mesh.glb'));
const mp4 = b64(join(dev, 'e2e/fixtures/interactive/media/clip.mp4'));
const webmB64 = b64(webm);
const stub = Buffer.from('GLB-STUB-NOT-A-MODEL\n').toString('base64');

// The harness page and a listener: any request other than the page itself is egress.
const hits = [];
const server = createServer((req, res) => {
  if (req.url === '/') {
    res.setHeader('content-type', 'text/html');
    res.end('<!doctype html><title>reader</title><body></body>');
  } else if (req.url === '/favicon.ico') {
    res.statusCode = 404;
    res.end();
  } else {
    hits.push(req.url);
    res.statusCode = 404;
    res.end();
  }
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const origin = `http://127.0.0.1:${server.address().port}`;

const browser = await firefox.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 640, height: 480 } });
const logs = [];
page.on('console', (m) => logs.push(m.text()));
await page.goto(`${origin}/`);

// In the page: a tiny reader. `mount` makes a frame, `send` posts an init with
// bytes, `events` records what each frame said, `look` decodes a snapshot.
await page.evaluate(() => {
  const frames = new Map();
  window.__events = [];
  window.addEventListener('message', (e) => {
    for (const [id, f] of frames) {
      if (e.source === f.contentWindow) {
        window.__events.push({ id, data: e.data, origin: e.origin });
        return;
      }
    }
  });
  const theme = (mode) => ({
    mode,
    tokens:
      mode === 'dark'
        ? { '--m-figure-bg': '#16181d', '--m-color-text': '#dfe1e6' }
        : { '--m-figure-bg': '#ffffff', '--m-color-text': '#1e1b24' },
  });
  window.__mount = (id, html, sandboxed = true) => {
    const f = document.createElement('iframe');
    f.id = id;
    if (sandboxed) f.setAttribute('sandbox', 'allow-scripts');
    f.style.cssText = 'width:480px;height:300px;border:0;display:block';
    f.srcdoc = html;
    frames.set(id, f);
    document.body.append(f);
  };
  window.__unmount = (id) => {
    document.getElementById(id)?.remove();
    frames.delete(id);
  };
  const bytes = (b64) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)).buffer;
  window.__init = (id, role, name, mime, b64, mode, options = {}) => {
    const buf = bytes(b64);
    const target = frames.get(id).contentWindow;
    const msg = {
      mfw: 1,
      type: 'init',
      protocol: 1,
      widgetId: id,
      runtime: 'x@1',
      alt: 'alt text',
      options,
      theme: theme(mode),
      sources: { [role]: { name, mime, sha256: '', bytes: buf } },
    };
    target.postMessage(msg, '*', [buf]);
  };
  window.__theme = (id, mode) =>
    frames.get(id).contentWindow.postMessage({ mfw: 1, type: 'theme', ...theme(mode) }, '*');
  window.__snap = (id, req) =>
    frames
      .get(id)
      .contentWindow.postMessage({ mfw: 1, type: 'snapshot-request', requestId: req }, '*');
  window.__look = async (png) => {
    const img = new Image();
    img.src = png;
    await img.decode();
    const c = document.createElement('canvas');
    c.width = img.width;
    c.height = img.height;
    const ctx = c.getContext('2d');
    ctx.drawImage(img, 0, 0);
    const d = ctx.getImageData(0, 0, c.width, c.height).data;
    const colors = new Set();
    let magenta = 0;
    for (let i = 0; i < d.length; i += 4) {
      colors.add((d[i] >> 4) * 256 + (d[i + 1] >> 4) * 16 + (d[i + 2] >> 4));
      if (d[i] > 240 && d[i + 1] < 20 && d[i + 2] > 240) magenta++;
    }
    return {
      w: img.width,
      h: img.height,
      colors: colors.size,
      corner: [d[0], d[1], d[2]],
      magenta,
    };
  };
  window.__hash = (png) => png.length + ':' + png.slice(-64);
});

const events = (id) => page.evaluate((i) => window.__events.filter((e) => e.id === i), id);
async function until(fn, ms = 20_000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const v = await fn();
    if (v) return v;
    await page.waitForTimeout(100);
  }
  return null;
}
const terminal = (id) =>
  until(async () =>
    (await events(id)).find((e) => e.data.type === 'status' && e.data.state !== 'loading'),
  );
async function snapshot(id) {
  const req = `r${Math.random().toString(36).slice(2)}`;
  await page.evaluate(([i, r]) => window.__snap(i, r), [id, req]);
  const ev = await until(async () =>
    (await events(id)).find((e) => e.data.type === 'snapshot' && e.data.requestId === req),
  );
  return ev?.data.png ?? null;
}

// Controls first: the harness can see a breach, and the grep sees a CDN.
await page.evaluate((html) => {
  window.__mount(
    'control',
    html.replace('</body>', '<script>try{parent.document.title="PWNED"}catch(e){}</script></body>'),
    false,
  );
}, '<!doctype html><body></body>');
check(
  'red control: an unsandboxed frame reads its parent',
  await until(async () => (await page.title()) === 'PWNED', 5000),
);
await page.evaluate(() => window.__unmount('control'));
await page.evaluate(() => (document.title = 'reader'));

const sandboxProbe = `<script>
 var r = {};
 try { r.parent = parent.document.title; } catch (e) { r.parent = 'blocked'; }
 try { r.storage = typeof localStorage; } catch (e) { r.storage = 'blocked'; }
 parent.postMessage({ mfw: 1, type: 'probe', r: r }, '*');
</script>`;
await page.evaluate(
  (h) => window.__mount('probe', h),
  withPolicy(`<!doctype html><head></head><body>${sandboxProbe}</body>`),
);
const probe = await until(async () => (await events('probe')).find((e) => e.data.type === 'probe'));
check(
  'the sandbox attribute alone stops parent access and storage',
  probe?.data.r.parent === 'blocked' && probe?.data.r.storage === 'blocked',
  JSON.stringify(probe?.data),
);
await page.evaluate(() => window.__unmount('probe'));

// ---- model ----
await page.evaluate((h) => window.__mount('m1', h), runtime('model'));
check(
  'model: announces ready',
  !!(await until(async () => (await events('m1')).find((e) => e.data.type === 'ready'))),
);
await page.evaluate(
  (b) => window.__init('m1', 'model', 'mesh.glb', 'model/gltf-binary', b, 'light'),
  glb,
);
const mt = await terminal('m1');
check(
  'model: loads the fixture glb offline inside the sandbox',
  mt?.data.state === 'loaded',
  JSON.stringify(mt?.data),
);
check(
  'model: speaks only from a null origin with mfw 1',
  (await events('m1')).every((e) => e.origin === 'null' && e.data.mfw === 1),
);
const light = await snapshot('m1');
const lookL = light ? await page.evaluate((p) => window.__look(p), light) : null;
check(
  'model: the picture is not blank (the cube is drawn)',
  !!lookL && lookL.colors >= 6,
  JSON.stringify(lookL),
);
check(
  'model: light background is the figure background',
  !!lookL && lookL.corner.every((v) => v === 255),
  JSON.stringify(lookL?.corner),
);
await page.evaluate(() => window.__theme('m1', 'dark'));
await page.waitForTimeout(300);
const dark = await snapshot('m1');
const lookD = dark ? await page.evaluate((p) => window.__look(p), dark) : null;
check(
  'model: dark theme changes the background',
  !!lookD && lookD.corner[0] < 60 && lookD.corner[2] < 60,
  JSON.stringify(lookD?.corner),
);
if (shotsAt && light)
  writeFileSync(join(shotsAt, 'model-light.png'), Buffer.from(light.split(',')[1], 'base64'));
if (shotsAt && dark)
  writeFileSync(join(shotsAt, 'model-dark.png'), Buffer.from(dark.split(',')[1], 'base64'));
// Orbit: drag across the frame, the picture must change.
const box = await page.locator('#m1').boundingBox();
await page.mouse.move(box.x + 240, box.y + 150);
await page.mouse.down();
await page.mouse.move(box.x + 340, box.y + 190, { steps: 8 });
await page.mouse.up();
await page.waitForTimeout(300);
const turned = await snapshot('m1');
check('model: dragging orbits the camera', !!turned && !!dark && turned !== dark);
if (shotsAt && turned)
  writeFileSync(join(shotsAt, 'model-orbit.png'), Buffer.from(turned.split(',')[1], 'base64'));

// A frame that is not the parent must not drive the runtime: a sibling posts
// a forged init (and a forged theme) to every frame on the page.
const before = (await events('m1')).length;
await page.evaluate(
  (h) => window.__mount('sib', h),
  withPolicy(`<!doctype html><head></head><body><script>
    var b = new ArrayBuffer(8);
    for (var i = 0; i < parent.frames.length; i++) {
      if (parent.frames[i] === window) continue;
      parent.frames[i].postMessage({ mfw: 1, type: 'init', protocol: 1, widgetId: 'forged', runtime: 'x@1', alt: 'forged', options: {},
        theme: { mode: 'dark', tokens: { '--m-figure-bg': '#ff00ff' } }, sources: { model: { name: 'x', mime: '', sha256: '', bytes: b } } }, '*');
      parent.frames[i].postMessage({ mfw: 1, type: 'theme', mode: 'dark', tokens: { '--m-figure-bg': '#ff00ff' } }, '*');
    }
  </script></body>`),
);
await page.waitForTimeout(800);
const after = await events('m1');
check(
  'model: forged init and theme from a sibling frame are ignored',
  after.length === before,
  `${before} -> ${after.length}`,
);
const snapAfter = await snapshot('m1');
const lookF = snapAfter ? await page.evaluate((p) => window.__look(p), snapAfter) : null;
check(
  'model: the forged theme left no magenta',
  !!lookF && lookF.magenta === 0,
  JSON.stringify(lookF),
);
await page.evaluate(() => window.__unmount('sib'));
await page.evaluate(() => window.__unmount('m1'));

// bad input: the stub text, and a truncated glb
await page.evaluate((h) => window.__mount('m2', h), runtime('model'));
await until(async () => (await events('m2')).find((e) => e.data.type === 'ready'));
await page.evaluate(
  (b) => window.__init('m2', 'model', 'mesh.glb', 'model/gltf-binary', b, 'light'),
  stub,
);
const bad = await terminal('m2');
check(
  'model: a file that is not a glb ends in error with a message',
  bad?.data.state === 'error' && /not a binary glTF/.test(bad.data.message ?? ''),
  JSON.stringify(bad?.data),
);
await page.evaluate(() => window.__unmount('m2'));

// ---- video ----
await page.evaluate((h) => window.__mount('v1', h), runtime('video'));
await until(async () => (await events('v1')).find((e) => e.data.type === 'ready'));
await page.evaluate(
  (b) => window.__init('v1', 'video', 'clip.webm', 'video/webm', b, 'light', { loop: true }),
  webmB64,
);
const vt = await terminal('v1');
check(
  'video: loads the clip from a blob url inside the sandbox',
  vt?.data.state === 'loaded',
  JSON.stringify(vt),
);
const vs = await until(() => snapshot('v1'));
const lookV = vs ? await page.evaluate((p) => window.__look(p), vs) : null;
check(
  'video: the frame on screen is not blank',
  !!lookV && lookV.colors >= 8,
  JSON.stringify(lookV),
);
if (shotsAt && vs)
  writeFileSync(join(shotsAt, 'video.png'), Buffer.from(vs.split(',')[1], 'base64'));
await page.evaluate(() => window.__unmount('v1'));

await page.evaluate((h) => window.__mount('v2', h), runtime('video'));
await until(async () => (await events('v2')).find((e) => e.data.type === 'ready'));
await page.evaluate((b) => window.__init('v2', 'video', 'clip.webm', '', b, 'light'), stub);
const vbad = await terminal('v2');
check(
  'video: bytes that are not a video end in error with a message',
  vbad?.data.state === 'error' && !!vbad.data.message,
  JSON.stringify(vbad),
);
await page.evaluate(() => window.__unmount('v2'));

// The fixture mp4 itself: a stock Firefox may lack H.264, so either outcome is a clean terminal status.
await page.evaluate((h) => window.__mount('v3', h), runtime('video'));
await until(async () => (await events('v3')).find((e) => e.data.type === 'ready'));
await page.evaluate((b) => window.__init('v3', 'video', 'clip.mp4', 'video/mp4', b, 'light'), mp4);
const v3 = await terminal('v3');
console.log(
  `info: fixture clip.mp4 in this Firefox -> ${v3?.data.state}${v3?.data.message ? ` (${v3.data.message})` : ''}`,
);
check('video: the fixture mp4 reaches a terminal status', !!v3);
await page.evaluate(() => window.__unmount('v3'));

check('nothing reached the network listener', hits.length === 0, hits.join(', '));
const csp = logs.filter((l) => /Content Security Policy/i.test(l));
console.log(`info: ${csp.length} CSP console reports`);

// Red control for the policy: a video runtime that loads a script and fetches
// from the listener. Under the policy neither request may land; with the
// policy removed both do, which proves the listener sees them.
const evil = (html) =>
  html.replace(
    '<body>',
    `<body><script src="${origin}/evil.js"></script><script>fetch("${origin}/evil-fetch").catch(function(){});new Image().src="${origin}/evil-img"</script>`,
  );
const raw = readFileSync(join(dev, 'src-tauri/widget-runtimes/video/index.html'), 'utf8');
await page.evaluate((h) => window.__mount('e1', h), withPolicy(evil(raw)));
await page.waitForTimeout(1500);
check(
  'a runtime that references an external url gets no request out under the policy',
  hits.length === 0,
  hits.join(', '),
);
await page.evaluate(() => window.__unmount('e1'));
await page.evaluate((h) => window.__mount('e2', h), evil(raw));
await until(async () => hits.length >= 2, 5000);
check(
  'red control: without the policy the same runtime reaches the listener',
  hits.some((h) => h === '/evil.js') && hits.some((h) => h === '/evil-fetch'),
  hits.join(', '),
);
await page.evaluate(() => window.__unmount('e2'));

await browser.close();
server.close();
rmSync(scratch, { recursive: true, force: true });
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
