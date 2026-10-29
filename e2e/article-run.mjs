// Article check: a headless browser (Firefox unless --browser says otherwise)
// loads the single-file export of the playground paper from file:// and
// drives the article's own interactivity. Proofs arrive as open <details>
// (open with scripts off), start folded with scripts on, toggle by click and
// keyboard, open for print, and open when the page lands on something inside
// them. Citations show their bibliography entry in a card on hover and on
// keyboard focus (no ids, no "Cited by" back-links), hide on leave and
// Escape, and still jump to the entry on click. Live equations show the
// PDF's values with scripts off. With scripts on an inline one gets
// controls (a slider recomputes, reset restores) and a mode=widget one
// mounts the liveeq@1 frame (a typed value is clamped, every copy of a
// param follows). No CSP violation throughout.
//
//   node e2e/article-run.mjs --single <file.html> [--browser chromium|firefox|webkit]
//
// e2e/playground-run.sh exports the bundle and calls this with it.
import { chromium, firefox, webkit } from 'playwright-core';
import { pathToFileURL } from 'node:url';

const arg = (name) => {
  const i = process.argv.indexOf(name);
  return i < 0 ? null : process.argv[i + 1];
};
const single = arg('--single');
const engines = { chromium, firefox, webkit };
const engineName = arg('--browser') ?? 'firefox';
if (!single || !engines[engineName]) {
  console.error(
    'usage: node e2e/article-run.mjs --single <file.html> [--browser chromium|firefox|webkit]',
  );
  process.exit(2);
}
const url = pathToFileURL(single).href;

let failed = 0;
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'ok' : 'FAIL'}: ${engineName}: ${name}${ok ? '' : ` -- ${detail}`}`);
  if (!ok) failed++;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const browser = await engines[engineName].launch();

async function open(path = '', opts = {}) {
  const ctx = await browser.newContext({ viewport: { width: 1000, height: 760 }, ...opts });
  await ctx.addInitScript(() => {
    window.__csp = [];
    document.addEventListener('securitypolicyviolation', (e) => {
      window.__csp.push(`${e.violatedDirective} ${e.blockedURI}`);
    });
  });
  const page = await ctx.newPage();
  await page.goto(url + path);
  return { ctx, page };
}

const proofsOf = (page) =>
  page.evaluate(() =>
    [...document.querySelectorAll('article details.ltx_proof')].map((d) => ({
      open: d.open,
      summary: d.querySelector(':scope > summary')?.textContent.trim() ?? '',
      qed: !!d.querySelector('.m-qed'),
    })),
  );

// ---- scripts off: proofs read open ----
{
  const { ctx, page } = await open('', { javaScriptEnabled: false });
  const p = await proofsOf(page);
  check(
    'scripts off: every proof is an open details with its title as the summary',
    p.length === 2 && p.every((x) => x.open && /^Proof/.test(x.summary)),
    JSON.stringify(p),
  );
  check(
    'scripts off: no div proof is left unfolded',
    (await page.locator('article div.ltx_proof').count()) === 0,
  );
  await ctx.close();
}

// ---- proofs ----
{
  const { ctx, page } = await open();
  let p = await proofsOf(page);
  check(
    'proofs start folded with scripts on',
    p.length === 2 && p.every((x) => !x.open),
    JSON.stringify(p),
  );
  check(
    "the author's proof title is the summary",
    p.some((x) => /^Proof of Theorem\s*\d+\.?$/.test(x.summary)),
    JSON.stringify(p.map((x) => x.summary)),
  );
  check(
    'each proof keeps its drawn end mark',
    p.every((x) => x.qed),
    JSON.stringify(p),
  );
  const first = page.locator('article details.ltx_proof > summary').first();
  await first.click();
  p = await proofsOf(page);
  const bodyShown = await page.evaluate(() => {
    const d = document.querySelector('article details.ltx_proof');
    const para = d.querySelector(':scope > .ltx_para');
    return !!para && para.getBoundingClientRect().height > 0;
  });
  check(
    'a click on the summary unfolds the proof',
    p[0].open && !p[1].open && bodyShown,
    JSON.stringify(p),
  );
  await first.click();
  check('a second click folds it again', !(await proofsOf(page))[0].open);
  await first.focus();
  await page.keyboard.press('Enter');
  check('Enter on the focused summary unfolds it', (await proofsOf(page))[0].open);
  await page.keyboard.press('Enter');
  check('Enter again folds it', !(await proofsOf(page))[0].open);

  await page.evaluate(() => window.dispatchEvent(new Event('beforeprint')));
  const printed = await proofsOf(page);
  await page.evaluate(() => window.dispatchEvent(new Event('afterprint')));
  const after = await proofsOf(page);
  check(
    'printing unfolds every proof and folds them back after',
    printed.every((x) => x.open) && after.every((x) => !x.open),
    JSON.stringify([printed, after]),
  );
  check('no csp violations on the article', (await page.evaluate(() => window.__csp)).length === 0);
  await ctx.close();
}

// ---- landing inside a proof opens it ----
{
  const probe = await open();
  const inner = await probe.page.evaluate(
    () =>
      document.querySelectorAll('article details.ltx_proof')[1]?.querySelector('[id]')?.id ?? '',
  );
  await probe.ctx.close();
  check('the second proof has an anchor inside it', !!inner);
  if (inner) {
    const { ctx, page } = await open('#' + encodeURIComponent(inner));
    const p = await proofsOf(page);
    check(
      'a page opened at an anchor inside a proof unfolds that proof only',
      !p[0].open && p[1].open,
      JSON.stringify(p),
    );
    await ctx.close();
  }
}

// ---- citation cards ----
{
  const { ctx, page } = await open();
  const cardState = () =>
    page.evaluate(() => {
      const c = document.getElementById('m-cite-card');
      if (!c) return null;
      const r = c.getBoundingClientRect();
      return {
        shown: !c.hidden && getComputedStyle(c).display !== 'none',
        text: c.textContent.replace(/\s+/g, ' ').trim(),
        ids: c.querySelectorAll('[id]').length,
        cited: c.querySelectorAll('.ltx_bib_cited').length,
        role: c.getAttribute('role'),
        inView: r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight,
        describedBy: [...document.querySelectorAll('[aria-describedby="m-cite-card"]')].length,
      };
    });
  // The citation link whose bibliography entry mentions `who`.
  const linkTo = (who, nth = 0) =>
    page.evaluateHandle(
      ([w, n]) =>
        [...document.querySelectorAll('article cite a[href^="#"]')].filter((a) => {
          const li = document.getElementById(a.getAttribute('href').slice(1));
          return li && li.textContent.includes(w);
        })[n] ?? null,
      [who, nth],
    );
  const cites = await page.locator('article cite a[href^="#"]').count();
  check('the article has citation links', cites >= 6, String(cites));
  let s = await cardState();
  check('the card starts hidden', !!s && !s.shown && s.role === 'tooltip', JSON.stringify(s));

  const knuth = (await linkTo('Knuth')).asElement();
  await knuth.scrollIntoViewIfNeeded();
  await knuth.hover();
  s = await cardState();
  check(
    'hovering a citation shows its entry in the card',
    s.shown && /TeXbook|T\s*e\s*X\s*book/i.test(s.text) && s.text.includes('Knuth'),
    JSON.stringify(s),
  );
  check(
    'the card copies no ids and no "Cited by" back-links',
    s.ids === 0 && s.cited === 0 && !/Cited by/.test(s.text),
    JSON.stringify(s),
  );
  check('the card sits inside the viewport', s.inView, JSON.stringify(s));
  check('the hovered link is described by the card', s.describedBy === 1, JSON.stringify(s));

  await page.mouse.move(5, 5);
  await sleep(400);
  s = await cardState();
  check('leaving the link hides the card', !s.shown && s.describedBy === 0, JSON.stringify(s));

  // In a multi-citation each link previews its own entry.
  const lamport = (await linkTo('Lamport', 1)).asElement() ?? (await linkTo('Lamport')).asElement();
  await lamport.hover();
  s = await cardState();
  check(
    'a multi-citation link previews its own entry',
    s.shown && s.text.includes('Lamport') && !s.text.includes('Knuth'),
    JSON.stringify(s),
  );
  await page.mouse.move(5, 5);
  await sleep(400);

  const cover = (await linkTo('Cover')).asElement();
  await cover.focus();
  s = await cardState();
  check(
    'keyboard focus on a citation shows the card',
    s.shown && s.text.includes('Cover'),
    JSON.stringify(s),
  );
  await page.keyboard.press('Escape');
  s = await cardState();
  check('Escape hides the card', !s.shown, JSON.stringify(s));

  const target = await knuth.getAttribute('href');
  await knuth.hover();
  await knuth.click();
  await sleep(100);
  s = await cardState();
  const hash = await page.evaluate(() => decodeURIComponent(location.hash));
  check(
    'a click still jumps to the entry and hides the card',
    !s.shown && hash === decodeURIComponent(target),
    JSON.stringify([hash, target, s.shown]),
  );
  check('no csp violations with the card', (await page.evaluate(() => window.__csp)).length === 0);
  await ctx.close();
}

// ---- live equations ----
const liveOf = (page) =>
  page.evaluate(() => {
    const m = document.querySelector('math.m-liveeq-eq-binary-entropy');
    const pm = document.querySelector('math.m-liveeq-eq-parabola');
    return {
      p: m?.querySelector('.m-param-p')?.textContent ?? null,
      h: m?.querySelector('.m-result-1')?.textContent ?? null,
      xs: pm ? [...pm.querySelectorAll('.m-param-x')].map((t) => t.textContent) : [],
      y: pm?.querySelector('.m-result-1')?.textContent ?? null,
      controls: document.querySelectorAll('.m-liveeq-controls').length,
    };
  });
{
  const { ctx, page } = await open('', { javaScriptEnabled: false });
  const s = await liveOf(page);
  check(
    "scripts off: live equations show the PDF's values and no controls",
    s.p === '0.25' && s.h === '0.811' && s.y === '2.25' && s.controls === 0,
    JSON.stringify(s),
  );
  await ctx.close();
}
{
  const { ctx, page } = await open();
  let s = await liveOf(page);
  check(
    'an inline live equation gets its controls under it, at the defaults',
    s.controls === 1 && s.p === '0.25' && s.h === '0.811',
    JSON.stringify(s),
  );
  const range = page.locator('.m-liveeq-controls input[type=range][aria-label="p"]');
  await range.fill('0.5');
  s = await liveOf(page);
  check(
    'moving the slider recomputes the result',
    s.p === '0.50' && s.h === '1.000',
    JSON.stringify(s),
  );
  await page.locator('.m-liveeq-reset').first().click();
  s = await liveOf(page);
  check('reset restores the defaults', s.p === '0.25' && s.h === '0.811', JSON.stringify(s));

  // mode=widget: the liveeq@1 frame mounts when near the screen, the
  // equation being its poster until it is ready.
  const fig = page.locator('figure[data-widget="eq-parabola"]');
  check('a widget-mode equation is a mount figure', (await fig.count()) === 1);
  await fig.scrollIntoViewIfNeeded();
  let state = null;
  for (let i = 0; i < 100 && state !== 'ready'; i++) {
    await sleep(100);
    state = await fig.getAttribute('data-state');
  }
  check('the liveeq@1 widget reaches ready', state === 'ready', String(state));
  const inner = page
    .frameLocator('figure[data-widget="eq-parabola"] iframe')
    .frameLocator('iframe');
  const box = inner.locator('input[type=number][aria-label="x value"]');
  await box.fill('-5');
  await box.press('Enter');
  await box.blur();
  const xs = await inner.locator('.m-param-x').allTextContents();
  const y = await inner.locator('.m-result-1').textContent();
  check(
    'in the widget, a typed value is clamped and every copy of the param follows',
    xs.length === 2 && xs.every((x) => x === '\u22122.0') && y === '4.00',
    JSON.stringify([xs, y]),
  );
  check(
    'no csp violations with live equations',
    (await page.evaluate(() => window.__csp)).length === 0,
  );
  await ctx.close();
}

await browser.close();
console.log(
  failed
    ? `ARTICLE RUN: ${failed} failed (${engineName})`
    : `ARTICLE RUN: all green (${engineName})`,
);
process.exit(failed ? 1 : 0);
