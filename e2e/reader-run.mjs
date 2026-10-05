// Reader check: a headless browser (Firefox unless --browser says otherwise) loads the index.html of an exported
// single-file bundle (over http and from file://) and of a folder bundle (over
// http; from file:// it must say it is unsupported). Asserts every widget frame
// is mounted once inside the reflowed article, sandbox="allow-scripts" exactly,
// every widget reaches ready (its poster is then replaced), the poster is
// shown with scripts off, the article itself (title, headings, one contents
// nav, footnotes collected into a list, the pdf offered as a download and not
// embedded), the reader's meta CSP, no request beyond the
// bundle's own files, and the spike's controls: an unsandboxed and an
// allow-same-origin frame read their parent (red), a widget that navigates its
// own frame off-site is stopped by the reader's CSP but not by a copy of the
// page with the CSP removed (red).
//
//   node e2e/reader-run.mjs --single <file.html> --folder <dir> [--browser chromium|firefox|webkit]
//
// e2e/export-run.sh produces both bundles and calls this with them. Needs a
// browser for playwright-core (npx playwright-core install firefox chromium webkit).
//
// With --embed-single, --embed-folder, --embed-ports A,B,U, --cert and --key it
// also runs the declared-origin cells over the two-widget embed variant:
// three https listeners on 127.0.0.1 (one self-signed certificate, which the
// browser context is told to accept) play fig-embed's widget.json origin (A),
// fig-macro's macro-option origin (B) and an origin nobody declared (U).
// Declared frames must load and render; U, and A framed by fig-macro, must
// not; fig-macro navigating its own frame to A must not either (the
// single-file reader's frame-src is the union, so a per-widget wrapper
// policy holds it). Copies of the bundles with each policy removed show
// which layer holds (red controls).
import { chromium, firefox, webkit } from 'playwright-core';
import { readFileSync, existsSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { createServer as createHttpsServer } from 'node:https';
import { extname, join, normalize } from 'node:path';
import { pathToFileURL } from 'node:url';

const arg = (name) => {
  const i = process.argv.indexOf(name);
  return i < 0 ? null : process.argv[i + 1];
};
const singleFile = arg('--single');
const customProof = arg('--custom-proof');
const embedSingle = arg('--embed-single');
const embedFolder = arg('--embed-folder');
const embedPorts = arg('--embed-ports');
const folderDir = arg('--folder');
const engines = { chromium, firefox, webkit };
const engineName = arg('--browser') ?? 'firefox';
if (!singleFile || !folderDir || !engines[engineName]) {
  console.error(
    'usage: node e2e/reader-run.mjs --single <file.html> --folder <dir> [--browser chromium|firefox|webkit]',
  );
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
const browser = await engines[engineName].launch({ headless: true });

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
    // Resolve the plate token through a dummy so hex and rgb compare equal.
    const dummy = document.createElement('div');
    dummy.style.background = 'var(--m-figure-bg)';
    document.body.append(dummy);
    const plateToken = getComputedStyle(dummy).backgroundColor;
    dummy.remove();
    return {
      order: figs.map((f) => f.dataset.widget),
      plateToken,
      frames: figs.map((f) => {
        const fr = f.querySelectorAll('iframe');
        return {
          count: fr.length,
          sandbox: fr[0]?.getAttribute('sandbox'),
          live: f.classList.contains('live'),
          state: f.dataset.state ?? '',
          posterShown: getComputedStyle(f.querySelector('.poster')).display !== 'none',
          caption:
            (
              f.querySelector('figcaption') ??
              f.parentElement?.closest('figure')?.querySelector(':scope > figcaption')
            )?.textContent ?? '',
          note: f.querySelector('.m-widget-note')?.textContent?.trim() ?? '',
          plate: (() => {
            // The plate is the .frame box inside the figure, not the
            // figure itself (which stays transparent).
            const bg = getComputedStyle(f.querySelector('.frame') ?? f).backgroundColor;
            const alpha = bg.startsWith('rgba') ? Number(bg.split(',').pop().replace(')', '')) : 1;
            return { bg, opaque: alpha === 1 };
          })(),
          mounted: !!(fr[0] && (fr[0].srcdoc || fr[0].getAttribute('src'))),
        };
      }),
      pdfLink: document.getElementById('pdf-link')?.getAttribute('href')?.slice(0, 30),
      pdfEmbedded: !!document.querySelector('object, embed, #pdf'),
      title: document.querySelector('article h1')?.textContent?.trim() ?? '',
      headings: [...document.querySelectorAll('article h2.ltx_title')].length,
      navCount: document.querySelectorAll('nav.m-contents').length,
      tocCount: document.querySelectorAll('.m-toc').length,
      navLinks: [...document.querySelectorAll('nav.m-contents a')].map((a) => ({
        ok: !!document.getElementById(a.getAttribute('href').slice(1)),
        text: a.textContent.trim(),
      })),
      navInArticleBeforeFigures: (() => {
        const n = document.querySelector('article nav.m-contents');
        const f = document.querySelector('figure[data-widget]');
        return !n || !f || !!(n.compareDocumentPosition(f) & Node.DOCUMENT_POSITION_FOLLOWING);
      })(),
      notesListed: document.querySelectorAll('.m-notes li').length,
      notesInline: document.querySelectorAll('.ltx_note.ltx_role_footnote').length,
      posters: figs.map((f) => {
        const i = f.querySelector('.poster');
        const r = i.getBoundingClientRect();
        const fr = f.querySelector('.frame').getBoundingClientRect();
        return {
          fit: getComputedStyle(i).objectFit,
          ar: f.style.getPropertyValue('--ar').trim(),
          framed: Math.abs(r.width - fr.width) < 2 && Math.abs(r.height - fr.height) < 2,
        };
      }),
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
  await ctx.addInitScript(() => {
    window.__csp = [];
    // Not counted: the favicon request.
    document.addEventListener('securitypolicyviolation', (e) => {
      if (e.blockedURI.endsWith('/favicon.ico')) return;
      window.__csp.push(`${e.violatedDirective} ${e.blockedURI}`);
    });
  });
  const page = await ctx.newPage();
  await page.goto(url);
  const all = await until(async () => {
    const s = await inspect(page);
    return s.frames.length && s.frames.every((f) => f.live || f.state === 'error') ? s : null;
  });
  const s = all ?? (await inspect(page));
  check(
    `${label}: every widget frame is mounted once, in the article`,
    JSON.stringify([...s.order].sort()) === JSON.stringify([...ids].sort()) &&
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
    `${label}: the pdf is offered as a download and not embedded`,
    !!s.pdfLink && !s.pdfEmbedded,
    JSON.stringify([s.pdfLink, s.pdfEmbedded]),
  );
  check(
    `${label}: the article has its title and section headings`,
    !!s.title && s.headings > 0,
    JSON.stringify([s.title, s.headings]),
  );
  // One section or none means the exporter deliberately omits the list
  // (pinned by the article unit tests); more than one is the
  // duplicate-contents bug. When present it is the article's own.
  check(
    `${label}: at most one contents list, the article's own, linking to real sections`,
    s.navCount <= 1 &&
      s.tocCount === 0 &&
      (s.navCount === 0 ||
        (s.navLinks.length > 0 && s.navLinks.every((l) => l.ok) && s.navInArticleBeforeFigures)),
    JSON.stringify([s.navCount, s.tocCount, s.navLinks.filter((l) => !l.ok)]),
  );
  check(
    `${label}: footnotes are collected into one list`,
    s.notesInline === 0 || s.notesListed === s.notesInline,
    JSON.stringify([s.notesListed, s.notesInline]),
  );
  check(
    `${label}: each poster fills its frame, contained, at the widget's aspect ratio`,
    s.posters.every((p) => p.fit === 'contain' && /^[\d.]+ \/ [\d.]+$/.test(p.ar)),
    JSON.stringify(s.posters),
  );
  check(
    `${label}: each widget has a caption (its own or its float's)`,
    s.frames.every((f) => f.caption.trim().length > 0),
  );
  check(
    `${label}: every widget frame sits on the opaque figure plate`,
    s.frames.length > 0 &&
      s.plateToken !== 'rgba(0, 0, 0, 0)' &&
      s.frames.every((f) => f.plate.opaque),
    JSON.stringify([s.plateToken, s.frames.map((f) => f.plate.bg)]),
  );
  // Chart hover tooltip: only pages with a chart widget have one to hover
  // (the embed variant has none, so absence there is a skip, not a pass).
  // Single-file nests the widget document inside a per-widget wrapper
  // frame, so scan every frame rather than the figure's own iframe.
  {
    let found = false;
    let shown = false;
    let text = '';
    for (const fr of page.frames()) {
      let has = false;
      try {
        has = (await fr.locator('#tip').count()) > 0;
      } catch {
        continue;
      }
      if (!has) continue;
      found = true;
      try {
        await fr.locator('#chart canvas').hover({ timeout: 10000 });
        await page.waitForTimeout(500);
        text = (await fr.locator('#tip').textContent()) ?? '';
        shown = !(await fr.locator('#tip[hidden]').count()) && text.trim().length > 0;
      } catch {
        /* detached mid-hover; shown stays false with the evidence below */
      }
      break;
    }
    check(
      `${label}: chart hover shows its tooltip`,
      found && shown,
      found ? JSON.stringify(text.slice(0, 120)) : 'no chart widget on this page',
    );
  }
  check(`${label}: the reader's meta CSP is the exporter's`, s.csp === expectCsp, s.csp);
  const violations = await page.evaluate(() => window.__csp);
  check(
    `${label}: the reader's own page breaks no CSP directive`,
    violations.length === 0,
    violations.join('; '),
  );
  return { ctx, page, s };
}

const fold = readFileSync(new URL('../src-tauri/core/src/bundle/fold.rs', import.meta.url), 'utf8');
const constant = (name) => new RegExp(`const ${name}: &str = "([^"]*)";`).exec(fold)[1];
const SINGLE_CSP = constant('SINGLE_FILE_READER_POLICY');
const FOLDER_CSP = constant('FOLDER_READER_POLICY');

// ---- the three loads ----
const sH = await reader('single-file over http', `${single.origin}/`, SINGLE_CSP);
check(
  'single-file over http: the pdf link is the inline data url',
  sH.s.pdfLink.startsWith('data:application/pdf'),
  sH.s.pdfLink,
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
  'folder over http: the pdf link points at paper.pdf',
  fH.s.pdfLink === 'paper.pdf',
  JSON.stringify(fH.s),
);
check(
  'folder over http: only bundle files were requested',
  folder.requests.every(
    (p) => p === '/' || p === '/favicon.ico' || /^\/(assets|widgets|figures)\//.test(p),
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

// ---- custom runtimes: one live widget, one fallback poster ----
// --custom-proof points at a proof bundle (bundle/tests.rs regenerates it
// with MALEFICIUM_RUNTIME_PROOF set): heatmap@1 approved and live,
// stl-viewer@1 missing and poster-only. bad-cdn@1 has no cell on purpose:
// a scan error makes the snapshot invalid, so it can never be approved
// and always exports poster-only (covered at the unit level).
if (customProof) {
  const proofHtml = readFileSync(customProof, 'utf8');
  const proofManifest = JSON.parse(
    /<script type="application\/json" id="mfw-manifest">(.*?)<\/script>/s.exec(proofHtml)[1],
  );
  check(
    'custom proof: the manifest carries the approved runtime entry',
    !!proofManifest.runtimes?.['heatmap@1']?.digest &&
      proofManifest.runtimes['heatmap@1'].license === 'MIT',
    Object.keys(proofManifest.runtimes ?? {}).join(','),
  );
  const proof = await host(join(customProof, '..'), customProof);
  const ctx = await browser.newContext();
  await ctx.addInitScript(() => {
    window.__csp = [];
    document.addEventListener('securitypolicyviolation', (e) => {
      window.__csp.push(`${e.violatedDirective} ${e.blockedURI}`);
    });
  });
  const page = await ctx.newPage();
  await page.goto(`${proof.origin}/`);
  const c = await until(async () => {
    const s = await inspect(page);
    return s.frames.length === 2 && s.frames.every((f) => f.live || f.state === 'poster-only')
      ? s
      : null;
  });
  check('custom proof: the page settles with two widgets', !!c, c ? '' : 'never settled');
  const live = (c?.frames ?? []).filter((f) => f.live);
  const posters = (c?.frames ?? []).filter((f) => f.state === 'poster-only');
  check(
    'custom proof: the approved runtime mounts live, sandboxed exactly allow-scripts',
    live.length === 1 && live[0].sandbox === 'allow-scripts' && !live[0].posterShown,
    JSON.stringify(live.map((f) => [f.sandbox, f.posterShown])),
  );
  check(
    'custom proof: the missing runtime stays poster-only with its note',
    posters.length === 1 &&
      posters[0].note.includes('stl-viewer@1') &&
      posters[0].note.includes('not installed'),
    JSON.stringify(posters.map((f) => f.note)),
  );
  check(
    'custom proof: the reader CSP is the single-file policy with no violations',
    c?.csp === SINGLE_CSP && (await page.evaluate(() => window.__csp)).length === 0,
    JSON.stringify([c?.csp?.slice(0, 60), await page.evaluate(() => window.__csp)]),
  );
  await ctx.close();
}

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

// ---- declared origins: an embed variant with two html widgets ----
if (embedSingle && embedFolder && embedPorts) {
  const tls = { cert: readFileSync(arg('--cert')), key: readFileSync(arg('--key')) };
  const [pa, pb, pu] = embedPorts.split(',').map(Number);
  // Each listener records the paths it served; a page it serves loads one
  // image from its own origin, so `<path>.rendered` proves the frame rendered.
  const hits = { A: [], B: [], U: [] };
  const origins = {};
  const listeners = [];
  for (const [name, port] of [
    ['A', pa],
    ['B', pb],
    ['U', pu],
  ]) {
    const server = createHttpsServer(tls, (req, res) => {
      const path = new URL(req.url, 'https://x').pathname;
      hits[name].push(path);
      if (path.endsWith('.rendered')) {
        res.setHeader('content-type', 'image/png');
        res.end(
          Buffer.from(
            'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==',
            'base64',
          ),
        );
        return;
      }
      res.setHeader('content-type', 'text/html');
      res.end(`<!doctype html><title>${name}</title><img src="${path}.rendered">`);
    });
    await new Promise((r) => server.listen(port, '127.0.0.1', r));
    listeners.push(server);
    origins[name] = `https://127.0.0.1:${port}`;
  }

  const CSP_META = /<meta http-equiv="Content-Security-Policy"[^>]*>/;
  // The reader script's wrapper policy for a single-file widget; its
  // directive name plus ` *` admits every origin.
  const WRAPPER_POLICY = "var WRAP_DIRECTIVE = 'frame-src";
  const noWrapper = (html) => html.replace(WRAPPER_POLICY, WRAPPER_POLICY + ' *');
  const noReaderCsp = (html) => html.replace(CSP_META, '');
  // Single-file: the widget documents ride in the mfw-widgets island.
  const noWidgetCsp = (html) =>
    html.replace(
      /(<script type="application\/json" id="mfw-widgets">)(.*?)(<\/script>)/s,
      (_, open, body, close) => {
        const docs = JSON.parse(body);
        for (const k of Object.keys(docs)) docs[k] = docs[k].replace(CSP_META, '');
        return open + JSON.stringify(docs).replace(/</g, '\\u003c') + close;
      },
    );
  const singleText = readFileSync(embedSingle, 'utf8');
  const singleVariants = {
    '/': singleText,
    '/nowidgetcsp.html': noWidgetCsp(singleText),
    '/noreadercsp.html': noReaderCsp(singleText),
    '/nocsp.html': noReaderCsp(noWidgetCsp(noWrapper(singleText))),
    // The reader policy without its frame-src: what a reader that ignored
    // the declared origins would ship.
    '/nounion.html': singleText.replace(/; frame-src [^"]*"/, '"'),
    // Each widget's wrapper document without its own policy: what a reader
    // that mounted widgets straight under the union would allow.
    '/nowrapper.html': noWrapper(singleText),
  };
  check(
    'embed: the wrapper-policy red control changes the reader',
    singleVariants['/nowrapper.html'] !== singleText,
  );
  const serve = (handler) => {
    const server = createServer((req, res) => {
      const r = handler(decodeURIComponent(new URL(req.url, 'http://x').pathname));
      if (!r) {
        res.statusCode = 404;
        res.end();
        return;
      }
      res.setHeader('content-type', r.type);
      res.end(r.body);
    });
    return new Promise((r) =>
      server.listen(0, '127.0.0.1', () =>
        r({ server, origin: `http://127.0.0.1:${server.address().port}` }),
      ),
    );
  };
  const eSingle = await serve((p) =>
    singleVariants[p] ? { type: 'text/html', body: singleVariants[p] } : null,
  );
  // Folder: under /nwc/ the same bundle is served with every widget
  // document's own policy removed (the reader page keeps its own).
  const eFolder = await serve((p) => {
    const strip = p.startsWith('/nwc/');
    let rel = strip ? p.slice(4) : p;
    if (rel === '/') rel = '/index.html';
    const file = join(embedFolder, normalize(rel));
    if (!file.startsWith(embedFolder) || !existsSync(file) || !statSync(file).isFile()) return null;
    const type = MIME[extname(file)] ?? 'application/octet-stream';
    if (strip && /^\/widgets\//.test(rel)) {
      return { type, body: readFileSync(file, 'utf8').replace(CSP_META, '') };
    }
    return { type, body: readFileSync(file) };
  });

  const seen = (o, path) => hits[o].includes(path);
  async function cell(label, url, expect) {
    for (const k of Object.keys(hits)) hits[k].length = 0;
    const ctx = await browser.newContext({ ignoreHTTPSErrors: true });
    const page = await ctx.newPage();
    await page.goto(url);
    const mounted = await until(async () => {
      const n = await page.evaluate(
        () =>
          [...document.querySelectorAll('figure[data-widget] iframe')].filter(
            (f) => f.srcdoc || f.getAttribute('src'),
          ).length,
      );
      return n === 2;
    });
    check(`${label}: both embed widgets are mounted`, !!mounted);
    // Wait for every frame that should load to render, then a quiet spell
    // for any that should not.
    const want = Object.entries(expect).filter(([, v]) => v);
    await until(
      async () => want.every(([k]) => seen(k.split(' ')[0], `${k.split(' ')[1]}.rendered`)),
      15_000,
    );
    await new Promise((r) => setTimeout(r, 2000));
    for (const [k, should] of Object.entries(expect)) {
      const [o, path] = k.split(' ');
      const got = seen(o, path);
      const rendered = seen(o, `${path}.rendered`);
      check(
        `${label}: ${path} on ${o} ${should ? 'loads and renders' : 'is blocked (no request)'}`,
        should ? got && rendered : !got && !rendered,
        JSON.stringify(hits),
      );
    }
    await ctx.close();
  }
  const GREEN = {
    'A /w1-declared': true,
    'U /w1-undeclared': false,
    'B /w2-declared': true,
    'A /w2-cross': false,
    'A /w2-selfnav': false, // the wrapper's frame-src names fig-macro's origin only
  };
  await cell('embed single-file over http', `${eSingle.origin}/`, GREEN);
  await cell('embed single-file from file://', pathToFileURL(embedSingle).href, GREEN);
  await cell('embed folder over http', `${eFolder.origin}/`, GREEN); // frame-src 'self'
  // Red controls: remove one policy at a time.
  await cell('embed single-file without widget CSPs', `${eSingle.origin}/nowidgetcsp.html`, {
    'A /w1-declared': true,
    'U /w1-undeclared': false, // the reader's frame-src, inherited by the srcdoc widget
    'B /w2-declared': true,
    'A /w2-cross': false, // the wrapper's policy, inherited too
    'A /w2-selfnav': false,
  });
  await cell('embed single-file without the reader CSP', `${eSingle.origin}/noreadercsp.html`, {
    'A /w1-declared': true,
    'U /w1-undeclared': false, // the widget's own policy and its wrapper's hold
    'B /w2-declared': true,
    'A /w2-cross': false,
    'A /w2-selfnav': false, // the wrapper alone holds
  });
  await cell('embed single-file without the wrapper policy', `${eSingle.origin}/nowrapper.html`, {
    'A /w1-declared': true,
    'U /w1-undeclared': false,
    'B /w2-declared': true,
    'A /w2-cross': false, // fig-macro's own policy
    'A /w2-selfnav': true, // red: the reader's union admits fig-embed's origin
  });
  await cell(
    'embed single-file whose reader lacks the frame-src union',
    `${eSingle.origin}/nounion.html`,
    {
      'A /w1-declared': false, // a srcdoc widget inherits the reader's policy
      'U /w1-undeclared': false,
      'B /w2-declared': false,
      'A /w2-cross': false,
      'A /w2-selfnav': false,
    },
  );
  await cell('embed single-file with no CSP at all', `${eSingle.origin}/nocsp.html`, {
    'A /w1-declared': true,
    'U /w1-undeclared': true, // red: the undeclared origin loads
    'B /w2-declared': true,
    'A /w2-cross': true,
    'A /w2-selfnav': true,
  });
  await cell('embed folder without widget CSPs', `${eFolder.origin}/nwc/`, {
    'A /w1-declared': true,
    'U /w1-undeclared': true, // red: a folder widget does not inherit the reader policy
    'B /w2-declared': true,
    'A /w2-cross': true,
    'A /w2-selfnav': false, // the folder reader's frame-src 'self'
  });
  for (const s of listeners) s.close();
  eSingle.server.close();
  eFolder.server.close();
} else if (embedSingle || embedFolder || embedPorts) {
  check('embed cells need --embed-single, --embed-folder and --embed-ports together', false);
}

await browser.close();
single.server.close();
folder.server.close();
leakServer.close();
console.log(failed ? `${failed} check(s) failed` : `READER PROOFS COMPLETE (${engineName})`);
process.exit(failed ? 1 : 0);
