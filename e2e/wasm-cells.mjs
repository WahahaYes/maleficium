// WASM reader cells (contract 2): a proof bundle with one WASM-declaring
// custom runtime (wasm-sum@1, live with 'wasm-unsafe-eval') and one plain
// control (heatmap@1, live without it) is loaded in one engine:
//
//   node e2e/wasm-cells.mjs --proof <proof.html> --browser chromium|firefox|webkit
//
// e2e/export-run.sh regenerates the proof (bundle/tests.rs writes it with
// MALEFICIUM_WASM_PROOF set) and runs this in all three engines. Needs a
// browser for playwright-core (npx playwright-core install ...).
//
// Cells: the declaring widget reaches live with its poster replaced, the
// control reaches live, only its document carries the token, the reader
// breaks no CSP directive, and only the bundle itself is requested. The
// gating probe then serves the same page with the token stripped from the
// declaring document only: engines that gate WASM (Chromium, Firefox) must
// leave that widget in error, engines that do not (WebKit) still run it.
import { chromium, firefox, webkit } from 'playwright-core';
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { join } from 'node:path';

const arg = (name) => {
  const i = process.argv.indexOf(name);
  return i < 0 ? null : process.argv[i + 1];
};
const proofFile = arg('--proof');
const engines = { chromium, firefox, webkit };
const engineName = arg('--browser') ?? 'firefox';
if (!proofFile || !engines[engineName]) {
  console.error(
    'usage: node e2e/wasm-cells.mjs --proof <proof.html> --browser chromium|firefox|webkit',
  );
  process.exit(2);
}

let failed = 0;
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'ok' : 'FAIL'}: ${name}${ok ? '' : ` -- ${detail}`}`);
  if (!ok) failed++;
};

const proofHtml = readFileSync(proofFile, 'utf8');
const manifest = JSON.parse(
  /<script type="application\/json" id="mfw-manifest">(.*?)<\/script>/s.exec(proofHtml)[1],
);
const wasmW = manifest.widgets.find((w) => w.runtime === 'wasm-sum@1') ?? {};
const plainW = manifest.widgets.find((w) => w.runtime === 'heatmap@1') ?? {};
check(
  'wasm proof: the manifest names the declaring and the control runtimes',
  !!wasmW.id && !!plainW.id,
  manifest.widgets.map((w) => `${w.id}:${w.runtime ?? w.type}`).join(','),
);
check(
  'wasm proof: the manifest records the declaration on one runtime only',
  manifest.runtimes?.['wasm-sum@1']?.capabilities?.wasm === true &&
    manifest.runtimes?.['heatmap@1']?.capabilities?.wasm === false,
  JSON.stringify(manifest.runtimes),
);

// The same page with the token stripped everywhere (the reader page
// inherits down into srcdoc widgets, so both layers must drop it): what a
// bundle with no declaring runtime ships.
const stripToken = (html) => html.replaceAll(" 'wasm-unsafe-eval'", '');
const noTokenHtml = stripToken(proofHtml);
const tokenCount = (h) => (h.match(/wasm-unsafe-eval/g) ?? []).length;
check(
  'gating probe: the proof carries the token and the stripped page none',
  tokenCount(proofHtml) >= 2 && tokenCount(noTokenHtml) === 0,
  `${tokenCount(proofHtml)} -> ${tokenCount(noTokenHtml)}`,
);

function host(variants) {
  const requests = [];
  const server = createServer((req, res) => {
    const url = new URL(req.url, 'http://x');
    requests.push(url.pathname);
    const body = variants[url.pathname];
    if (body === undefined) {
      res.statusCode = 404;
      res.end();
      return;
    }
    res.setHeader('content-type', 'text/html');
    res.end(body);
  });
  return new Promise((r) =>
    server.listen(0, '127.0.0.1', () =>
      r({ server, requests, origin: `http://127.0.0.1:${server.address().port}` }),
    ),
  );
}

const folded = await host({ '/': proofHtml, '/notoken.html': noTokenHtml });
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

const inspect = (page) =>
  page.evaluate(
    ([wasmId, plainId]) => {
      const fig = (id) => document.querySelector(`figure[data-widget="${id}"]`);
      const state = (f) => ({
        live: f?.classList.contains('live') ?? null,
        state: f?.dataset.state ?? null,
        sandbox: f?.querySelector(':scope iframe')?.getAttribute('sandbox') ?? null,
        posterShown:
          f == null ? null : getComputedStyle(f.querySelector('.poster')).display !== 'none',
        note: f?.querySelector('.m-widget-note')?.textContent?.trim() ?? null,
      });
      const island = JSON.parse(document.getElementById('mfw-widgets').textContent);
      return {
        wasm: state(fig(wasmId)),
        plain: state(fig(plainId)),
        settled: [wasmId, plainId].every((id) => {
          const f = fig(id);
          return f && (f.classList.contains('live') || f.dataset.state === 'error');
        }),
        wasmToken: island[wasmId].includes('wasm-unsafe-eval'),
        plainToken: island[plainId].includes('wasm-unsafe-eval'),
        csp: document.querySelector('meta[http-equiv="Content-Security-Policy"]')?.content,
      };
    },
    [wasmW.id, plainW.id],
  );

async function load(path) {
  const ctx = await browser.newContext();
  await ctx.addInitScript(() => {
    window.__csp = [];
    document.addEventListener('securitypolicyviolation', (e) => {
      if (e.blockedURI.endsWith('/favicon.ico')) return;
      window.__csp.push(`${e.violatedDirective} ${e.blockedURI}`);
    });
  });
  const page = await ctx.newPage();
  await page.goto(`${folded.origin}${path}`);
  const s = await until(async () => {
    const v = await inspect(page);
    return v.settled ? v : null;
  });
  const end = s ?? (await inspect(page));
  const violations = await page.evaluate(() => window.__csp);
  return { ctx, s: end, violations };
}

// ---- the declaring widget and its control, as shipped ----
{
  const { ctx, s, violations } = await load('/');
  check('wasm proof: the page settles with both widgets decided', !!s?.settled, JSON.stringify(s));
  check(
    'wasm proof: the declaring runtime mounts live, sandboxed exactly allow-scripts',
    s?.wasm.live === true && s?.wasm.sandbox === 'allow-scripts' && s?.wasm.posterShown === false,
    JSON.stringify(s?.wasm),
  );
  check(
    'wasm proof: the control mounts live with its poster replaced',
    s?.plain.live === true && s?.plain.posterShown === false,
    JSON.stringify(s?.plain),
  );
  check(
    'wasm proof: only the declaring document carries the token',
    s?.wasmToken === true && s?.plainToken === false,
    `wasm=${s?.wasmToken} plain=${s?.plainToken}`,
  );
  check(
    'wasm proof: the reader breaks no CSP directive',
    violations.length === 0,
    violations.join('; '),
  );
  await ctx.close();
}

// ---- the gating probe: the same WASM code without the token ----
{
  // Every engine here gates WebAssembly compilation on the token
  // (Chromium, Firefox, and this WebKit build; the old WebKitGTK 2.52
  // note predates upstream gating). Without it the widget must error.
  // A mismatch fails the run: the per-engine gating claim needs a look.
  const { ctx, s, violations } = await load('/notoken.html');
  check(
    'gating probe: the page settles with both widgets decided',
    !!s?.settled,
    JSON.stringify(s),
  );
  check(
    `gating probe (${engineName}): the control still mounts live`,
    s?.plain.live === true,
    JSON.stringify(s?.plain),
  );
  check(
    `gating probe (${engineName}): without the token the WASM widget errors`,
    s?.wasm.state === 'error' && s?.wasm.live !== true,
    JSON.stringify(s?.wasm),
  );
  check(
    'gating probe: the reader breaks no CSP directive',
    violations.length === 0,
    violations.join('; '),
  );
  await ctx.close();
}

check(
  'no request beyond the bundle itself was made',
  folded.requests
    .filter((p) => p !== '/favicon.ico')
    .every((p) => p === '/' || p === '/notoken.html'),
  folded.requests.join(','),
);

await browser.close();
folded.server.close();
console.log(failed ? `${failed} check(s) failed` : `WASM PROOFS COMPLETE (${engineName})`);
process.exit(failed ? 1 : 0);
