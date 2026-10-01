// The paper-snippet MCP App View: the region of the compiled PDF a `snippet`
// call landed on, beside the source that made it. One self-contained page, no
// framework. The image arrives as MCP image content, or is fetched with the
// app-only `snippet_render` tool when the model did not ask for one.
import {
  App,
  applyDocumentTheme,
  applyHostFonts,
  applyHostStyleVariables,
  type McpUiHostContext,
} from '@modelcontextprotocol/ext-apps';
import type { CallToolResult } from '@modelcontextprotocol/client';

interface Region {
  x: number;
  y: number;
  width: number;
  height: number;
}
interface SnippetData {
  main: string;
  page: number;
  pages: number;
  region: Region | null;
  stale: boolean;
  source: { rel: string; line: number; firstLine: number; excerpt: string } | null;
}
interface Ref {
  rootId: string;
  main: string;
}

const POLL_MS = 2000;

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;
const [bar, msg, paper, src, where, staleTag] = [
  'bar',
  'msg',
  'paper',
  'src',
  'where',
  'stale',
].map((id) => $(id));
const img = $<HTMLImageElement>('img');
const [prev, next, zoom] = ['prev', 'next', 'zoom'].map((id) => $<HTMLButtonElement>(id));

const app = new App({ name: 'maleficium-snippet', version: '1.0.0' });

let ref: Ref | null = null;
let data: SnippetData | null = null;
let stamp: string | null = null;
let wholePage = false;
let renderSeq = 0;

function say(text: string): void {
  msg.textContent = text;
  msg.hidden = false;
}

function imageOf(result: CallToolResult): string | null {
  const block = result.content.find((c) => c.type === 'image');
  return block && block.type === 'image' ? `data:${block.mimeType};base64,${block.data}` : null;
}

function show(d: SnippetData, image: string | null): void {
  data = d;
  bar.hidden = false;
  msg.hidden = true;
  where.textContent = `${d.main} · page ${d.page} of ${d.pages}`;
  staleTag.hidden = !d.stale;
  prev.disabled = d.page <= 1;
  next.disabled = d.page >= d.pages;
  zoom.hidden = !d.region;
  zoom.textContent = wholePage ? 'Region' : 'Whole page';
  if (image) {
    img.src = image;
    paper.hidden = false;
  } else {
    paper.hidden = true;
  }
  if (d.source) {
    const lines = d.source.excerpt.split('\n');
    src.replaceChildren(
      ...lines.map((text, i) => {
        const row = document.createElement('div');
        const n = d.source!.firstLine + i;
        if (n === d.source!.line) row.className = 'hit';
        row.textContent = `${String(n).padStart(4)}  ${text}`;
        return row;
      }),
    );
    src.hidden = false;
  } else {
    src.hidden = true;
  }
}

// Fetch the picture for `page` (the region band, or the whole page).
async function render(page: number, withRegion: boolean): Promise<void> {
  if (!ref || !data) return;
  const seq = ++renderSeq;
  try {
    const r = await app.callServerTool({
      name: 'snippet_render',
      arguments: {
        root_id: ref.rootId,
        main_rel: ref.main,
        page,
        ...(withRegion && data.region ? { region: data.region } : {}),
      },
    });
    if (seq !== renderSeq) return;
    if (r.isError) return say(textOf(r));
    const meta = r.structuredContent as { page: number; pages: number } | undefined;
    const same = withRegion && data.region && page === data.page;
    show(
      { ...data, page, pages: meta?.pages ?? data.pages, region: same ? data.region : null },
      imageOf(r),
    );
    // Moving off the target's page drops the region and the source highlight.
    if (page !== data.page && data.source) src.hidden = true;
  } catch (e) {
    if (seq === renderSeq) say(`Could not render: ${String(e)}`);
  }
}

function textOf(r: CallToolResult): string {
  const t = r.content.find((c) => c.type === 'text');
  return t && t.type === 'text' ? t.text : 'The tool failed.';
}

app.ontoolinput = (params) => {
  const a = params.arguments as { root_id?: string; main_rel?: string } | undefined;
  if (a?.root_id && a.main_rel) ref = { rootId: a.root_id, main: a.main_rel };
};

app.ontoolresult = async (result) => {
  if (result.isError) return say(textOf(result as CallToolResult));
  const d = result.structuredContent as unknown as SnippetData | undefined;
  if (!d) return say('The snippet had no content.');
  wholePage = false;
  const given = imageOf(result as CallToolResult);
  show(d, given);
  void pollStamp(true);
  if (!given) await render(d.page, true);
};

prev.onclick = () => data && void render(data.page - 1, false);
next.onclick = () => data && void render(data.page + 1, false);
zoom.onclick = () => {
  if (!data) return;
  wholePage = !wholePage;
  void render(data.page, !wholePage);
};

// Re-render when a compile (anyone's) replaces the pdf. The page is polled
// only while visible; pushed events replace this when the core has a sink.
async function pollStamp(seed = false): Promise<void> {
  if (!ref || document.visibilityState !== 'visible') return;
  try {
    const r = await app.callServerTool({
      name: 'output_stamp',
      arguments: { root_id: ref.rootId, main_rel: ref.main },
    });
    const s = JSON.stringify(
      (r.structuredContent as { stamp: unknown } | undefined)?.stamp ?? null,
    );
    const moved = stamp !== null && s !== stamp;
    stamp = s;
    if (moved && !seed && data) {
      data = { ...data, stale: false };
      await render(data.page, !wholePage && !!data.region);
    }
  } catch {
    /* host closed or tool busy: try again next tick */
  }
}
setInterval(() => void pollStamp(), POLL_MS);

function adopt(ctx: McpUiHostContext | undefined): void {
  if (!ctx) return;
  if (ctx.theme) applyDocumentTheme(ctx.theme);
  if (ctx.styles?.variables) applyHostStyleVariables(ctx.styles.variables);
  if (ctx.styles?.css?.fonts) applyHostFonts(ctx.styles.css.fonts);
}
app.onhostcontextchanged = adopt;

app
  .connect()
  .then(() => adopt(app.getHostContext()))
  .catch((e) => say(`Not connected to a host: ${String(e)}`));
