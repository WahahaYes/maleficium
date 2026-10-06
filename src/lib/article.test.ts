import { describe, it, expect, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import { fetchArticle } from './article.tauri';
import {
  ARTICLE_SANDBOX,
  articleAvailable,
  articleCspOf,
  articleSandbox,
  articleZoomLabel,
  clampArticleZoom,
  hasTauriInternals,
  isArticleMessage,
  stepArticleZoom,
} from './article';

// The reader e2e probes pin sandbox="allow-scripts" exactly: an unsandboxed
// or allow-same-origin frame reads its parent (red), allow-scripts alone
// cannot (green). The Article tab mounts the same shape in-app.
describe('article sandbox', () => {
  it('is allow-scripts exactly, never allow-same-origin', () => {
    expect(articleSandbox()).toBe('allow-scripts');
    expect(ARTICLE_SANDBOX).not.toContain('allow-same-origin');
  });
});

describe('article CSP', () => {
  it('reads the reader policy intact from the bytes', () => {
    const html =
      '<!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="default-src \'none\'"></head>' +
      '<body><article class="ltx_document"><h1>T</h1></article></body></html>';
    expect(articleCspOf(html)).toBe("default-src 'none'");
  });
  it('is null when the bytes carry no policy', () => {
    expect(articleCspOf('<html><body><article>plain</article></body></html>')).toBeNull();
  });
});

describe('bridge denial', () => {
  const readerBytes =
    '<!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="default-src \'none\'">' +
    '</head><body><article class="ltx_document"><h1>T</h1></article>' +
    '<script>window.addEventListener("message",function(e){})</script></body></html>';
  it('finds no bridge tokens in reader-shaped bytes', () => {
    expect(hasTauriInternals(readerBytes)).toBe(false);
  });
  it.each([
    ['internals object', '<script>window.__TAURI_INTERNALS__.invoke("x")</script>'],
    ['metadata object', '<script>window.__TAURI_METADATA__.x</script>'],
    ['legacy bridge', '<script>window.__TAURI__.invoke("core_request")</script>'],
    ['custom scheme', '<a href="tauri://localhost/x">x</a>'],
    ['ipc host', '<!-- http://ipc.localhost -->'],
  ])('flags the red control: %s', (_label, hostile) => {
    expect(hasTauriInternals(readerBytes + hostile)).toBe(true);
  });
});

describe('frame message gate', () => {
  const frame = { __frame: true };
  const good = () => ({ mfw: 1, type: 'approve-widget', widgetId: 'fig-demo' });
  it('accepts only the exact approve shape from the frame at origin null', () => {
    expect(isArticleMessage(good(), 'null', frame, frame)).toBe(true);
  });
  it.each([
    ['foreign source', [good(), 'null', { __other: true }, frame]],
    ['no source', [good(), 'null', null, frame]],
    ['non-null origin', [good(), 'https://x.example', frame, frame]],
    ['empty origin', [good(), '', frame, frame]],
    ['missing marker', [{ type: 'approve-widget', widgetId: 'fig-demo' }, 'null', frame, frame]],
    [
      'wrong marker',
      [{ mfw: 2, type: 'approve-widget', widgetId: 'fig-demo' }, 'null', frame, frame],
    ],
    ['wrong type', [{ mfw: 1, type: 'ready', widgetId: 'fig-demo' }, 'null', frame, frame]],
    ['missing widget', [{ mfw: 1, type: 'approve-widget' }, 'null', frame, frame]],
    ['empty widget', [{ mfw: 1, type: 'approve-widget', widgetId: '' }, 'null', frame, frame]],
    ['non-string widget', [{ mfw: 1, type: 'approve-widget', widgetId: 7 }, 'null', frame, frame]],
    ['null body', [null, 'null', frame, frame]],
    ['string body', ['approve-widget', 'null', frame, frame]],
  ])('drops the hostile message: %s', (_label, args) => {
    const [data, origin, source, expected] = args as [unknown, string, unknown, unknown];
    expect(isArticleMessage(data, origin, source, expected)).toBe(false);
  });
});

describe('article availability', () => {
  it('needs bytes and no running compile', () => {
    expect(articleAvailable('<html></html>', false)).toBe(true);
    expect(articleAvailable(null, false)).toBe(false);
    expect(articleAvailable('', false)).toBe(false);
    expect(articleAvailable('<html></html>', true)).toBe(false);
  });
});

describe('article zoom', () => {
  it('clamps into range and labels plainly', () => {
    expect(clampArticleZoom(100)).toBe(100);
    expect(clampArticleZoom(9)).toBe(50);
    expect(clampArticleZoom(400)).toBe(200);
    expect(clampArticleZoom(Number.NaN)).toBe(100);
    expect(articleZoomLabel(125)).toBe('125%');
  });
  it('steps one notch at a time, clamped', () => {
    expect(stepArticleZoom(100, 'in')).toBe(125);
    expect(stepArticleZoom(100, 'out')).toBe(75);
    expect(stepArticleZoom(200, 'in')).toBe(200);
    expect(stepArticleZoom(50, 'out')).toBe(50);
  });
});

describe('article transport', () => {
  it('names a project and main file only, and carries no path back', async () => {
    const view = {
      html: '<html><body><article>read me</article></body></html>',
      anchors: [{ id: 'S1', text: '1 First' }],
    };
    vi.mocked(invoke).mockResolvedValueOnce(view);
    expect(await fetchArticle('1a2b3c4d', 'main.tex')).toEqual(view);
    expect(invoke).toHaveBeenLastCalledWith('article_bundle', {
      rootId: '1a2b3c4d',
      mainRel: 'main.tex',
    });
  });
  it('passes the core refusal through (no pdf: compile first; unknown root)', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('main.tex has no compiled pdf: compile it first');
    await expect(fetchArticle('1a2b3c4d', 'main.tex')).rejects.toMatch(/compile it first/);
    vi.mocked(invoke).mockRejectedValueOnce('unknown project');
    await expect(fetchArticle('nope', 'main.tex')).rejects.toMatch(/unknown project/);
  });
});
