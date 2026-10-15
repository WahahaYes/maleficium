// Article frame check: the in-app Article tab's frame under the app window's
// real CSP (src-tauri/tauri.conf.json, app.security.csp). A host page served
// with that policy loads the reader page into a sandbox="allow-scripts"
// frame the way ArticleView does: from its own URL (the maleficium-article
// scheme, here its Windows http://maleficium-article.localhost form), where
// the reader's own CSP meta governs it. Every widget figure must mount, with
// no CSP violation. The control loads the same bytes as srcdoc, which
// inherits the window's policy: the reader's inline script is refused and
// nothing mounts. If the control ever mounts, this check no longer proves
// why the frame needs its own URL.
//
//   node e2e/article-frame-run.mjs --single <file.html> [--browser chromium|firefox|webkit]
//
// e2e/playground-run.sh exports the bundle and calls this with it.
import { chromium, firefox, webkit } from 'playwright-core';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const arg = (name) => {
  const i = process.argv.indexOf(name);
  return i < 0 ? null : process.argv[i + 1];
};
const single = arg('--single');
const engines = { chromium, firefox, webkit };
const engineName = arg('--browser') ?? 'chromium';
if (!single || !engines[engineName]) {
  console.error(
    'usage: node e2e/article-frame-run.mjs --single <file.html> [--browser chromium|firefox|webkit]',
  );
  process.exit(2);
}
const html = readFileSync(single, 'utf8');
const conf = JSON.parse(
  readFileSync(
    join(dirname(fileURLToPath(import.meta.url)), '../src-tauri/tauri.conf.json'),
    'utf8',
  ),
);
const csp = Object.entries(conf.app.security.csp)
  .map(([k, v]) => `${k} ${v}`)
  .join('; ');

let failed = 0;
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'ok' : 'FAIL'}: ${engineName}: ${name}${ok ? '' : ` -- ${detail}`}`);
  if (!ok) failed++;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const APP = 'http://app.localhost';
const ARTICLE = 'http://maleficium-article.localhost';
// The host script is external: the window policy refuses inline scripts.
const HOST_JS = `
  var f = document.createElement('iframe');
  f.setAttribute('sandbox', 'allow-scripts');
  f.setAttribute('referrerpolicy', 'no-referrer');
  f.style.cssText = 'width:1000px;height:4000px;border:0';
  window.mount = function (how, v) { if (how === 'src') f.src = v; else f.srcdoc = v; };
  document.body.appendChild(f);`;

const browser = await engines[engineName].launch();

/** Mounts the reader page as `how` ('src' | 'srcdoc'); returns figure states and violations. */
async function run(how) {
  const ctx = await browser.newContext({ viewport: { width: 1000, height: 760 } });
  const violations = [];
  ctx.on('console', (m) => {
    if (/content security policy|refused to/i.test(m.text()))
      violations.push(m.text().slice(0, 200));
  });
  await ctx.route(`${APP}/**`, (r) => {
    const path = new URL(r.request().url()).pathname;
    if (path === '/host.js')
      return r.fulfill({ status: 200, contentType: 'text/javascript', body: HOST_JS });
    return r.fulfill({
      status: 200,
      contentType: 'text/html',
      headers: { 'content-security-policy': csp },
      body: '<!doctype html><meta charset="utf-8"><body style="margin:0"><script src="/host.js"></script>',
    });
  });
  await ctx.route(`${ARTICLE}/**`, (r) =>
    r.fulfill({
      status: 200,
      contentType: 'text/html; charset=utf-8',
      headers: { 'cache-control': 'no-store' },
      body: html,
    }),
  );
  await ctx.addInitScript(() => {
    document.addEventListener('securitypolicyviolation', (e) => {
      console.log(`Content Security Policy: ${e.violatedDirective} ${e.blockedURI}`);
    });
  });
  const page = await ctx.newPage();
  await page.goto(`${APP}/`);
  await page.evaluate(
    ([how, v]) => window.mount(how, v),
    [how, how === 'src' ? `${ARTICLE}/0123456789abcdef0123456789abcdef` : html],
  );
  // The reader frame is the one holding the figures. Widgets mount lazily,
  // as they come near the screen, so each figure is scrolled into view.
  let states = [];
  for (let i = 0; i < 40; i++) {
    await sleep(250);
    for (const f of page.frames()) {
      if (f === page.mainFrame()) continue;
      const s = await f
        .evaluate(() =>
          Array.from(document.querySelectorAll('figure[data-widget]')).map((x) => [
            x.id,
            x.getAttribute('data-state'),
          ]),
        )
        .catch(() => []);
      if (s.length) states = s;
      const next = s.findIndex(([, st]) => st === 'deferred');
      if (next >= 0) {
        await f
          .locator('figure[data-widget]')
          .nth(next)
          .scrollIntoViewIfNeeded()
          .catch(() => {});
      }
    }
    const settled =
      states.length && states.every(([, st]) => st && st !== 'loading' && st !== 'deferred');
    if (settled || (how === 'srcdoc' && i >= 12)) break;
  }
  await ctx.close();
  return { states, violations };
}

const url = await run('src');
check('the reader page has widget figures', url.states.length > 0, 'no figure[data-widget] found');
const unmounted = url.states.filter(([, st]) => !st || st === 'deferred');
check(
  'every figure mounts in a frame loaded from its own url',
  url.states.length > 0 && unmounted.length === 0,
  JSON.stringify(url.states),
);
check(
  'some widget runs live',
  url.states.some(([, st]) => st === 'ready'),
  JSON.stringify(url.states),
);
check('no csp violation from its own url', url.violations.length === 0, url.violations.join(' | '));

const doc = await run('srcdoc');
check(
  'control: as srcdoc the window policy stops the reader script',
  doc.states.length > 0 && doc.states.every(([, st]) => !st) && doc.violations.length > 0,
  `${JSON.stringify(doc.states)} ${doc.violations.length} violations`,
);

await browser.close();
process.exit(failed ? 1 : 0);
