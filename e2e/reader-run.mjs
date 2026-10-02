// Companion-reader check: headless Firefox loads the index.html of an exported
// single-file bundle (over http and from file://) and of a folder bundle (over
// http; from file:// it must say it is unsupported). Asserts every widget frame
// is mounted in manifest order under the pdf, sandbox="allow-scripts" exactly,
// every widget reaches ready (its poster is then replaced), the poster is
// shown with scripts off, the reader's meta CSP, no request beyond the
// bundle's own files, and the spike's controls: an unsandboxed and an
// allow-same-origin frame read their parent (red), a widget that navigates its
// own frame off-site is stopped by the reader's CSP but not by a copy of the
// page with the CSP removed (red).
//
//   node e2e/reader-run.mjs --single <file.html> --folder <dir>
//
// e2e/export-run.sh produces both bundles and calls this with them. Needs a
// Firefox for playwright-core (npx playwright install firefox).
import { firefox } from 'playwright-core';
import { readFileSync, existsSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, normalize } from 'node:path';
import { pathToFileURL } from 'node:url';

const arg = (name) => {
  const i = process.argv.indexOf(name);
  return i < 0 ? null : process.argv[i + 1];
};
const singleFile = arg('--single');
const folderDir = arg('--folder');
if (!singleFile || !folderDir) {
  console.error('usage: node e2e/reader-run.mjs --single <file.html> --folder <dir>');
  process.exit(2);
}

let failed = 0;
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'ok' : 'FAIL'}: ${name}${ok ? '' : ` -- ${detail}`}`);
  if (!ok) failed++;
};

const MIME = {
  '.html': 'text/html',
  '.json': 'application/json',
  '.pdf': 'application/pdf',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.css': 'text/css',
};

// One static host per bundle, plus a listener that must never be reached.
// `nocsp` serves the same index.html with its reader CSP meta removed.
const leaks = [];
const leakServer = createServer((req, res) => {
  leaks.push(req.url);
  res.statusCode = 404;
  res.end();
});
await new Promise((r) => leakServer.listen(0, '127.0.0.1', r));
const leakOrigin = `http://127.0.0.1:${leakServer.address().port}`;

function host(root, indexFile) {
  const requests = [];
  const server = createServer((req, res) => {
    const url = new URL(req.url, 'http://x');
    requests.push(url.pathname);
    let rel = decodeURIComponent(url.pathname);
    let body;
    let type = 'text/html';
    if (rel === '/' || rel === '/index.html' || rel === '/nocsp.html') {
      body = readFileSync(indexFile, 'utf8');
      if (rel === '/nocsp.html') {
        body = body.replace(/<meta http-equiv="Content-Security-Policy"[^>]*>/, '');
      }
    } else {
      const p = join(root, normalize(rel));
      if (!p.startsWith(root) || !existsSync(p) || !statSync(p).isFile()) {
        res.statusCode = 404;
        res.end();
        return;
      }
      body = readFileSync(p);
      type = MIME[extname(p)] ?? 'application/octet-stream';
    }
    res.setHeader('content-type', type);
    res.end(body);
  });
  return new Promise((r) =>
    server.listen(0, '127.0.0.1', () =>
      r({ server, requests, origin: `http://127.0.0.1:${server.address().port}` }),
    ),
  );
}

const singleHtml = readFileSync(singleFile, 'utf8');
const manifestOf = (html) =>
  JSON.parse(/<script type="application\/json" id="mfw-manifest">(.*?)<\/script>/s.exec(html)[1]);
const manifest = manifestOf(singleHtml);
const ids = manifest.widgets.map((w) => w.id);
const folderManifest = JSON.parse(readFileSync(join(folderDir, 'manifest.json'), 'utf8'));
check(
  'both bundles list the same widgets in the same order',
  JSON.stringify(folderManifest.widgets.map((w) => w.id)) === JSON.stringify(ids) && ids.length > 0,
  ids.join(','),
);

const singleDir = join(singleFile, '..');
const single = await host(singleDir, singleFile);
const folder = await host(folderDir, join(folderDir, 'index.html'));
const browser = await firefox.launch({ headless: true });

async function until(fn, ms = 30_000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const v = await fn();
    if (v) return v;
    await new Promise((r) => setTimeout(r, 150));
  }
  return null;
}

// What a loaded reader looks like, read from its live DOM.
const inspect = (page) =>
  page.evaluate(() => {
    const figs = [...document.querySelectorAll('figure[data-widget]')];
    return {
      order: figs.map((f) => f.dataset.widget),
      frames: figs.map((f) => {
        const fr = f.querySelectorAll('iframe');
        return {
          count: fr.length,
          sandbox: fr[0]?.getAttribute('sandbox'),
          live: f.classList.contains('live'),
          state: f.dataset.state ?? '',
          posterShown: getComputedStyle(f.querySelector('.poster')).display !== 'none',
          caption: f.querySelector('figcaption')?.textContent ?? '',
          mounted: !!(fr[0] && (fr[0].srcdoc || fr[0].getAttribute('src'))),
        };
      }),
      pdfLink: document.getElementById('pdf-link')?.getAttribute('href')?.slice(0, 30),
      pdfObject: document.getElementById('pdf')?.getAttribute('data') ?? '',
      pdfBeforeFigures:
        !!document.getElementById('pdf') &&
        !!(
          document
            .getElementById('pdf')
            .compareDocumentPosition(document.querySelector('figure[data-widget]')) &
          Node.DOCUMENT_POSITION_FOLLOWING
        ),
      csp: document.querySelector('meta[http-equiv="Content-Security-Policy"]')?.content,
      title: document.title,
      unsupported: document.querySelector('.unsupported')?.textContent ?? '',
      anyUnsafeFrame: [...document.querySelectorAll('iframe')].some((f) =>
        (f.getAttribute('sandbox') ?? '').includes('allow-same-origin'),
      ),
    };
  });

async function reader(label, url, expectCsp) {
  const ctx = await browser.newContext({ viewport: { width: 900, height: 700 } });
  const page = await ctx.newPage();
  await page.goto(url);
  const all = await until(async () => {
    const s = await inspect(page);
    return s.frames.length && s.frames.every((f) => f.live || f.state === 'error') ? s : null;
  });
  const s = all ?? (await inspect(page));
  check(
    `${label}: every widget frame is mounted, in manifest order`,
    JSON.stringify(s.order) === JSON.stringify(ids) &&
      s.frames.every((f) => f.count === 1 && f.mounted),
    JSON.stringify(s.order),
  );
  check(
    `${label}: every frame is sandbox="allow-scripts" exactly`,
    s.frames.every((f) => f.sandbox === 'allow-scripts') && !s.anyUnsafeFrame,
    JSON.stringify(s.frames.map((f) => f.sandbox)),
  );
  check(
    `${label}: every widget reaches ready and its poster is replaced`,
    s.frames.every((f) => f.live && !f.posterShown),
    JSON.stringify(s.frames.map((f) => [f.live, f.state, f.posterShown])),
  );
  check(
    `${label}: no widget ends in an error status`,
    s.frames.filter((f) => f.state === 'error').length === 0,
    JSON.stringify(s.frames.map((f) => f.state)),
  );
  check(
    `${label}: the pdf link and embed come before the figures`,
    s.pdfBeforeFigures && !!s.pdfLink,
    JSON.stringify([s.pdfLink, s.pdfBeforeFigures]),
  );
  check(
    `${label}: each widget has a caption`,
    s.frames.every((f) => f.caption.trim().length > 0),
  );
  check(`${label}: the reader's meta CSP is the exporter's`, s.csp === expectCsp, s.csp);
  return { ctx, page, s };
}

const fold = readFileSync(new URL('../src-tauri/core/src/bundle/fold.rs', import.meta.url), 'utf8');
const constant = (name) => new RegExp(`const ${name}: &str = "([^"]*)";`).exec(fold)[1];
const SINGLE_CSP = constant('SINGLE_FILE_READER_POLICY');
const FOLDER_CSP = constant('FOLDER_READER_POLICY');

// ---- the three loads ----
const sH = await reader('single-file over http', `${single.origin}/`, SINGLE_CSP);
check(
  'single-file over http: the pdf object gets a blob url from the inline link',
  sH.s.pdfLink.startsWith('data:application/pdf') &&
    (await sH.page.evaluate(() => document.getElementById('pdf').data.startsWith('blob:'))),
  sH.s.pdfObject,
);
check(
  'single-file over http: the page is one request (plus a favicon)',
  single.requests.filter((p) => p !== '/favicon.ico').length === 1,
  single.requests.join(','),
);
await sH.ctx.close();

const sF = await reader('single-file from file://', pathToFileURL(singleFile).href, SINGLE_CSP);
await sF.ctx.close();

const fH = await reader('folder over http', `${folder.origin}/`, FOLDER_CSP);
check(
  'folder over http: the pdf object and link point at paper.pdf',
  fH.s.pdfObject === 'paper.pdf' && fH.s.pdfLink === 'paper.pdf',
  JSON.stringify(fH.s),
);
check(
  'folder over http: only bundle files were requested',
  folder.requests.every(
    (p) =>
      p === '/' || p === '/favicon.ico' || p === '/paper.pdf' || /^\/(assets|widgets)\//.test(p),
  ),
  folder.requests.join(','),
);
await fH.ctx.close();

const fF = await browser.newContext();
const fFP = await fF.newPage();
await fFP.goto(pathToFileURL(join(folderDir, 'index.html')).href);
const fFs = await until(async () => {
  const s = await inspect(fFP);
  return s.unsupported ? s : null;
}, 5000);
check(
  'folder from file://: the page says it is unsupported and shows no frame',
  !!fFs && /http/.test(fFs.unsupported) && (await fFP.locator('iframe').count()) === 0,
  fFs?.unsupported,
);
await fF.close();

// ---- scripts off: the posters and captions stay ----
{
  const ctx = await browser.newContext({ javaScriptEnabled: false });
  const page = await ctx.newPage();
  await page.goto(`${single.origin}/`);
  const posters = await page.evaluate(() =>
    [...document.querySelectorAll('figure[data-widget] .poster')].map(
      (i) => getComputedStyle(i).display !== 'none' && i.complete && i.naturalWidth > 0,
    ),
  );
  check(
    'scripts off: every widget shows its poster and no frame is mounted',
    posters.length === ids.length &&
      posters.every(Boolean) &&
      (await page.locator('iframe').count()) === 0,
    JSON.stringify(posters),
  );
  check(
    'scripts off: the noscript note is in the page and the pdf link is a data url',
    singleHtml.includes('<noscript>') &&
      (await page.getAttribute('#pdf-link', 'href')).startsWith('data:application/pdf'),
  );
  await ctx.close();
}

// ---- controls: the browser enforces what the page asks for ----
async function probes(origin, label, expectNavLeak) {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await page.goto(`${origin}`);
  await until(async () => (await inspect(page)).frames.every((f) => f.live));
  const readsParent = (sandbox) =>
    page.evaluate(async (sb) => {
      const f = document.createElement('iframe');
      if (sb !== null) f.setAttribute('sandbox', sb);
      f.srcdoc = '<script>try{parent.document.title="PWNED"}catch(e){}<' + '/script>';
      document.body.append(f);
      for (let i = 0; i < 40; i++) {
        if (document.title === 'PWNED') break;
        await new Promise((r) => setTimeout(r, 100));
      }
      const hit = document.title === 'PWNED';
      document.title = 'reader';
      f.remove();
      return hit;
    }, sandbox);
  check(`${label}: red control, an unsandboxed frame reads its parent`, await readsParent(null));
  check(
    `${label}: red control, allow-scripts allow-same-origin reads its parent`,
    await readsParent('allow-scripts allow-same-origin'),
  );
  check(
    `${label}: green, allow-scripts alone cannot read its parent`,
    !(await readsParent('allow-scripts')),
  );
  check(
    `${label}: the real widget frames are opaque to the reader`,
    await page.evaluate(() =>
      [...document.querySelectorAll('figure[data-widget] iframe')].every((f) => {
        try {
          return f.contentDocument === null;
        } catch {
          return true;
        }
      }),
    ),
  );
  // A sandboxed widget that navigates its own frame off-site.
  const before = leaks.length;
  await page.evaluate((u) => {
    const f = document.createElement('iframe');
    f.setAttribute('sandbox', 'allow-scripts');
    f.srcdoc =
      `<script>setTimeout(function(){location.href=${JSON.stringify(u)}},50)<` + '/script>';
    document.body.append(f);
  }, `${leakOrigin}/nav`);
  await new Promise((r) => setTimeout(r, 2500));
  const leaked = leaks.length - before;
  check(
    expectNavLeak
      ? `${label}: red control, without the reader CSP a widget navigates off-site`
      : `${label}: the reader CSP stops a widget navigating its frame off-site`,
    expectNavLeak ? leaked > 0 : leaked === 0,
    `${leaked} requests`,
  );
  await ctx.close();
}
await probes(`${single.origin}/`, 'single-file', false);
await probes(`${single.origin}/nocsp.html`, 'single-file without CSP', true);
await probes(`${folder.origin}/`, 'folder', false);
await probes(`${folder.origin}/nocsp.html`, 'folder without CSP', true);

check(
  'no request reached the egress listener from the readers or their widgets',
  leaks.every((u) => u === '/nav'),
  leaks.join(','),
);

await browser.close();
single.server.close();
folder.server.close();
leakServer.close();
console.log(failed ? `${failed} check(s) failed` : 'READER PROOFS COMPLETE');
process.exit(failed ? 1 : 0);
