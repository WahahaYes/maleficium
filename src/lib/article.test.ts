import { describe, it, expect, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import { fetchArticle } from './article.tauri';
import {
  ARTICLE_SANDBOX,
  anchorForLine,
  anchorMatchesSection,
  articleAvailable,
  articleCspOf,
  articleModeMessage,
  articleSandbox,
  articleScrollMessage,
  articleZoomLabel,
  clampArticleZoom,
  hasTauriInternals,
  isArticleMessage,
  resolveArticleMode,
  shouldRefreshArticle,
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

describe('editor-to-article anchor sync', () => {
  const sections = [
    { line: 1, title: 'First' },
    { line: 20, title: 'Second' },
    { line: 40, title: 'Deep dive' },
  ];
  const anchors = [
    { id: 'S1', text: '1 First' },
    { id: 'S2', text: '2 Second' },
    { id: 'S2-1', text: '2.1 Deep dive' },
  ];
  it('maps the caret line to its section anchor through the number prefix', () => {
    expect(anchorForLine(1, sections, anchors)).toBe('S1');
    expect(anchorForLine(19, sections, anchors)).toBe('S1');
    expect(anchorForLine(20, sections, anchors)).toBe('S2');
    expect(anchorForLine(99, sections, anchors)).toBe('S2-1');
  });
  it('is null with no anchors, no sections, or titles the article lacks', () => {
    expect(anchorForLine(5, sections, [])).toBeNull();
    expect(anchorForLine(5, [], anchors)).toBeNull();
    expect(anchorForLine(5, [{ line: 1, title: 'Elsewhere' }], anchors)).toBeNull();
  });
  it('falls back to the parent when the caret sits in a flattened subsection', () => {
    const deep = [...sections, { line: 45, title: 'Detail' }];
    expect(anchorForLine(46, deep, anchors)).toBe('S2-1');
  });
  it('takes the Nth same-titled anchor for the Nth same-titled section', () => {
    const dup = [
      { line: 1, title: 'Notes' },
      { line: 20, title: 'Notes' },
    ];
    const dupAnchors = [
      { id: 'S1', text: '1 Notes' },
      { id: 'S2', text: '2 Notes' },
    ];
    expect(anchorForLine(2, dup, dupAnchors)).toBe('S1');
    expect(anchorForLine(21, dup, dupAnchors)).toBe('S2');
  });
  it('matches exact titles and ignores blank ones on both sides', () => {
    expect(anchorMatchesSection('First', 'First')).toBe(true);
    expect(anchorMatchesSection('1 First', 'First')).toBe(true);
    expect(anchorMatchesSection('Firstborn', 'First')).toBe(false);
    expect(anchorMatchesSection('', 'First')).toBe(false);
    expect(anchorMatchesSection('1 First', '')).toBe(false);
  });
  it('builds the exact scroll message the bytes answer', () => {
    expect(articleScrollMessage('S2')).toEqual({ mfw: 1, type: 'article-scroll', id: 'S2' });
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

describe('article auto-refresh', () => {
  it('reloads shown bytes when a newer compile stamp lands', () => {
    expect(shouldRefreshArticle('<html></html>', 1, 2, false, true)).toBe(true);
  });
  it.each([
    ['never loaded', [null, 0, 1, false, true]],
    ['empty bytes', ['', 0, 1, false, true]],
    ['same stamp', ['<html></html>', 2, 2, false, true]],
    ['older stamp', ['<html></html>', 3, 2, false, true]],
    ['no stamp yet', ['<html></html>', 0, null, false, true]],
    ['compiling', ['<html></html>', 1, 2, true, true]],
    ['nothing to load from', ['<html></html>', 1, 2, false, false]],
  ])('stays put while %s', (_label, args) => {
    const [html, loaded, stamp, compiling, canLoad] = args as [
      string | null,
      number,
      number | null,
      boolean,
      boolean,
    ];
    expect(shouldRefreshArticle(html, loaded, stamp, compiling, canLoad)).toBe(false);
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

describe('article colour mode', () => {
  it('follows the app on system and keeps an explicit choice', () => {
    expect(resolveArticleMode('system', true)).toBe('dark');
    expect(resolveArticleMode('system', false)).toBe('light');
    expect(resolveArticleMode('light', true)).toBe('light');
    expect(resolveArticleMode('dark', false)).toBe('dark');
    expect(articleModeMessage('dark')).toEqual({ mfw: 1, type: 'article-mode', mode: 'dark' });
  });
});
