// Renders one snippet tool call in the MCP App View, headless, for an
// agent-run.py oracle: the same host page and AppBridge as run.mjs, against
// the server the agent used, on the project it left behind.
//
//   node e2e/apps-host/render.mjs <spec.json>
//
// spec.json: { cmd: [argv], env: {..}, project, args: {snippet arguments},
// shot: png path, out: json path, theme?: 'light' | 'dark' }. The server's
// HOME must already hold the compiled pdf (the judge compiles first). Writes
// to `out`: { where, src, img: [w, h], renderCalls, structured, error }.
import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
import { firefox } from 'playwright-core';
import { build } from 'vite';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const spec = JSON.parse(readFileSync(process.argv[2], 'utf8'));
const out = { where: null, src: null, img: [0, 0], renderCalls: 0, structured: null, error: null };
const scratch = mkdtempSync(join(tmpdir(), 'maleficium-apps-render-'));
let browser, server, client;

try {
  client = new Client({ name: 'apps-host-render', version: '0' });
  await client.connect(
    new StdioClientTransport({ command: spec.cmd[0], args: spec.cmd.slice(1), env: spec.env }),
  );
  await client.callTool({ name: 'grant', arguments: { root_id: 'view', root: spec.project } });
  const args = { ...spec.args, root_id: 'view' };
  const toolResult = await client.callTool({ name: 'snippet', arguments: args });
  if (toolResult.isError) throw new Error(`snippet: ${JSON.stringify(toolResult.content)}`);
  out.structured = toolResult.structuredContent;
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
  server = createServer((req, res) => {
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
  await new Promise((r) => server.once('listening', r));

  browser = await firefox.launch();
  const page = await browser.newPage();
  page.on('pageerror', (e) => (out.error ??= `page error: ${e}`));
  await page.exposeFunction('mcpCall', async (params) => {
    if (params.name === 'snippet_render') out.renderCalls++;
    return client.callTool({ name: params.name, arguments: params.arguments });
  });
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.evaluate((o) => window.runView(o), {
    html: viewHtml,
    toolInput: { arguments: args },
    toolResult,
    theme: spec.theme ?? 'light',
  });
  const view = page.frameLocator('#view');
  const img = view.locator('#img');
  await img.waitFor({ state: 'visible', timeout: 20000 }).catch(() => {});
  out.img = await img.evaluate((e) => [e.naturalWidth, e.naturalHeight]).catch(() => [0, 0]);
  out.where = await view.locator('#where').textContent();
  out.src = await view.locator('#src').textContent();
  if (out.where && out.where.includes(spec.project))
    out.error ??= 'the View shows an absolute path';
  mkdirSync(dirname(spec.shot), { recursive: true });
  await page.screenshot({ path: spec.shot });
} catch (e) {
  out.error = String(e && e.stack ? e.stack : e);
} finally {
  await browser?.close();
  server?.close();
  await client?.close();
  rmSync(scratch, { recursive: true, force: true });
}
writeFileSync(spec.out, JSON.stringify(out, null, 2));
process.exit(out.error ? 1 : 0);
