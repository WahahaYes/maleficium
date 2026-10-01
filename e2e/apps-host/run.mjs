// Headless host verification for the MCP App Views (latex-project mcp.4):
// drives the real maleficium-mcp over stdio, plays a chat client in headless
// Firefox, and checks what a View shows with and without the model asking
// for an image, and what it does when the user pages or a compile lands.
//
//   node e2e/apps-host/run.mjs [--shots <dir>]
//
// Needs a Firefox for playwright-core (npx playwright install firefox) and a
// built maleficium-mcp (cargo build --bin maleficium-mcp).
import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
import { firefox } from 'playwright-core';
import { build } from 'vite';
import { cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const dev = resolve(here, '../..');
const bin = join(
  process.env.CARGO_TARGET_DIR ?? join(dev, 'src-tauri/target'),
  'debug/maleficium-mcp',
);
const shotsAt = process.argv.includes('--shots')
  ? process.argv[process.argv.indexOf('--shots') + 1]
  : null;

let failed = 0;
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'ok' : 'FAIL'}: ${name}${ok ? '' : ` -- ${detail}`}`);
  if (!ok) failed++;
};

const scratch = mkdtempSync(join(tmpdir(), 'maleficium-apps-host-'));
const proj = join(scratch, 'proj');
cpSync(join(dev, 'e2e/fixtures/simple'), proj, { recursive: true });

const client = new Client({ name: 'apps-host-test', version: '0' });
// The engine cache is shared with driver-run.sh, so a warm machine compiles
// offline; app data stays in the scratch dir.
const env = {
  ...process.env,
  XDG_CACHE_HOME:
    process.env.DRIVER_CACHE ??
    join(process.env.TMPDIR ?? '/tmp', `maleficium-driver-cache-${process.getuid()}`),
  XDG_DATA_HOME: join(scratch, 'data'),
};
await client.connect(new StdioClientTransport({ command: bin, args: [], env }));
const call = async (name, args) => client.callTool({ name, arguments: args });
const text = (r) =>
  r.content
    .filter((c) => c.type === 'text')
    .map((c) => c.text)
    .join('');

await call('grant', { root_id: 'h', root: proj });
const job = (await call('compile_run', { root_id: 'h', rel: 'main.tex' })).structuredContent.job_id;
for (let i = 0; i < 600; i++) {
  const s = (await call('compile_poll', { job_id: job, tail_lines: 1 })).structuredContent;
  if (s.status !== 'running') {
    check('fixture compiles', s.status === 'success', JSON.stringify(s).slice(0, 200));
    break;
  }
  await new Promise((r) => setTimeout(r, 500));
}

const viewHtml = (await client.readResource({ uri: 'ui://maleficium/snippet/v1' })).contents[0]
  .text;

const hostDir = join(scratch, 'host');
await build({
  root: here,
  logLevel: 'error',
  build: {
    outDir: hostDir,
    emptyOutDir: true,
    target: 'es2020',
    rollupOptions: { input: join(here, 'host.html') },
  },
});
// A host page needs an origin: serve the build over file:// is blocked for modules.
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
const server = createServer((req, res) => {
  const f = join(hostDir, req.url === '/' ? 'host.html' : req.url.split('?')[0]);
  try {
    const body = readFileSync(f);
    res.setHeader('content-type', f.endsWith('.js') ? 'text/javascript' : 'text/html');
    res.end(body);
  } catch {
    res.statusCode = 404;
    res.end();
  }
}).listen(0);
const url = `http://127.0.0.1:${server.address().port}/`;

const browser = await firefox.launch();
const toolCalls = [];

async function openView(withImage, theme) {
  const page = await browser.newPage();
  page.on('pageerror', (e) => check('no page error', false, String(e)));
  await page.exposeFunction('mcpCall', async (params) => {
    toolCalls.push(params.name);
    return call(params.name, params.arguments);
  });
  await page.goto(url);
  const args = {
    root_id: 'h',
    main_rel: 'main.tex',
    label: 'fig:diagram',
    ...(withImage ? { with_image: true } : {}),
  };
  const toolResult = await call('snippet', args);
  await page.evaluate((o) => window.runView(o), {
    html: viewHtml,
    toolInput: { arguments: args },
    toolResult,
    theme,
  });
  return { page, view: page.frameLocator('#view'), toolResult };
}

for (const withImage of [true, false]) {
  const label = withImage ? 'model asked for an image' : 'no image in the tool result';
  toolCalls.length = 0;
  const { page, view, toolResult } = await openView(withImage, 'light');
  const img = view.locator('#img');
  await img.waitFor({ state: 'visible', timeout: 15000 }).catch(() => {});
  const size = await img.evaluate((e) => [e.naturalWidth, e.naturalHeight]).catch(() => [0, 0]);
  check(`${label}: the View shows the page region`, size[0] > 100 && size[1] > 20, String(size));
  check(
    `${label}: the render tool is called only when the result had no image`,
    toolCalls.includes('snippet_render') === !withImage,
    String(toolCalls),
  );
  const where = await view.locator('#where').textContent();
  const sc = toolResult.structuredContent;
  check(
    `${label}: the header names main and the page`,
    where.includes('main.tex') && where.includes(`page ${sc.page} of ${sc.pages}`),
    where,
  );
  const src = await view.locator('#src').textContent();
  check(
    `${label}: the source pane shows the label's lines`,
    src.includes('fig:diagram'),
    src.slice(0, 120),
  );
  check(
    `${label}: the View carries no absolute path`,
    !(await view.locator('body').innerHTML()).includes(proj),
  );
  if (withImage) {
    const ev = await view.locator('#zoom').isVisible();
    check('whole-page toggle is offered when there is a region', ev === !!sc.region, String(ev));
    if (sc.region) {
      const zoom = view.locator('#zoom');
      await zoom.click();
      await view
        .locator('#zoom', { hasText: 'Back to region' })
        .waitFor({ timeout: 5000 })
        .catch(() => {});
      check(
        'the whole-page view can go back to the region',
        (await zoom.isVisible()) && (await zoom.textContent()) === 'Back to region',
        await zoom.textContent(),
      );
      await zoom.click();
      await view
        .locator('#zoom', { hasText: 'Whole page' })
        .waitFor({ timeout: 5000 })
        .catch(() => {});
      check(
        'and the region toggle offers the whole page again',
        (await zoom.textContent()) === 'Whole page',
        await zoom.textContent(),
      );
    }
    // Paging asks the app-only tool for another page, with no model turn.
    toolCalls.length = 0;
    const pages = sc.pages;
    if (pages > 1) {
      const dir = sc.page < pages ? '#next' : '#prev';
      await view.locator(dir).click();
      await page.waitForTimeout(1500);
      const w2 = await view.locator('#where').textContent();
      check(
        'paging re-renders through snippet_render',
        toolCalls.includes('snippet_render') && !w2.includes(`page ${sc.page} of`),
        `${toolCalls} ${w2}`,
      );
    }
  }
  await page.close();
}

for (const theme of ['light', 'dark']) {
  const { page, view } = await openView(true, theme);
  await view
    .locator('#img')
    .waitFor({ state: 'visible', timeout: 15000 })
    .catch(() => {});
  const bg = await view.locator('body').evaluate((e) => getComputedStyle(e).backgroundColor);
  check(
    `${theme}: the frame follows the host theme`,
    theme === 'dark' ? bg !== 'rgb(255, 255, 255)' : bg === 'rgb(255, 255, 255)',
    bg,
  );
  const paper = await view.locator('.paper').evaluate((e) => getComputedStyle(e).backgroundColor);
  check(`${theme}: the page stays white paper`, paper === 'rgb(255, 255, 255)', paper);
  if (shotsAt) {
    mkdirSync(shotsAt, { recursive: true });
    await page.screenshot({ path: join(shotsAt, `snippet-${theme}.png`) });
  }
  await page.close();
}

// A compile that moves the pdf refreshes a visible View on its own.
{
  toolCalls.length = 0;
  const { page, view } = await openView(true, 'light');
  await view.locator('#img').waitFor({ state: 'visible', timeout: 15000 });
  await page.waitForTimeout(2500);
  toolCalls.length = 0;
  writeFileSync(
    join(proj, 'main.tex'),
    (await call('read', { root_id: 'h', rel: 'main.tex' })).structuredContent.text + '\n%touch\n',
  );
  const j = (await call('compile_run', { root_id: 'h', rel: 'main.tex' })).structuredContent.job_id;
  for (let i = 0; i < 240; i++) {
    if (
      (await call('compile_poll', { job_id: j, tail_lines: 1 })).structuredContent.status !==
      'running'
    )
      break;
    await new Promise((r) => setTimeout(r, 500));
  }
  await page.waitForTimeout(5000);
  check(
    'a recompile makes the View re-render',
    toolCalls.includes('snippet_render'),
    String(toolCalls),
  );
  await page.close();
}

await browser.close();
server.close();
await client.close();
rmSync(scratch, { recursive: true, force: true });
console.log(failed ? `\nAPPS HOST: ${failed} FAILED` : '\nAPPS HOST PROOFS COMPLETE');
process.exit(failed ? 1 : 0);
