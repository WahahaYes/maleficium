# Custom widget runtimes

A custom runtime is a small self-contained page that draws one interactive widget inside an exported paper bundle. The author writes `\interactiveruntime[runtime=stl-viewer@1, ...]{models/part.stl}`; the paper still compiles to PDF (the widget shows its poster), and the web bundle mounts the runtime live next to the poster.

## Folder layout

Project copy (the only one export reads): `runtimes/<name>@<major>/`. A user library copy installs from there. Layout:

```
runtimes/caption-overlay@1/
  runtime.json        REQUIRED, <= 64 KiB
  index.html          REQUIRED, the entry; classic scripts/styles only
  app.js, style.css   optional own code, referenced from index.html
  vendor/<lib>/...    third-party code plus its licence text
  samples/<file>      >= 1 file per required role (never exported)
  README.md, LICENSE  optional docs (never exported)
```

Limits: no symlinks, regular files only, <= 4096 files, <= 64 MiB walked. `runtime.json`, `samples/**`, readmes, licences, and each vendored `licenseFile` are metadata: carried for review, never folded into the page. Everything else is inlined into `widgets/<id>/index.html`, which runs in a `sandbox="allow-scripts"` frame with no network, no `eval`, no storage, no workers, and no module `import`.

## runtime.json

`contract` is exactly `1`. `name` matches the folder before `@` (`^[a-z][a-z0-9-]{1,39}$`, not a built-in or `m-*` name); `version` is `MAJOR.MINOR.PATCH` whose major matches the folder. `title` (1-80 chars), `description` (0-500), `authors` (1-16), `license` from the allowlist (MIT, BSD-2-Clause, BSD-3-Clause, Apache-2.0, ISC, Zlib, 0BSD, CC0-1.0, Unlicense). `sources` has 1-8 roles `{primary?: bool, required: bool, extensions: [...], maxBytes?: int, description?}` with exactly one `primary: true` that is also required. `options` is an optional closed object schema (<= 32 string/number/integer/ boolean properties with defaults, enums, minimum/maximum, maxLength). `capabilities` is `{webgl: bool}`. `vendored` lists third-party libraries (name, version, allowlist licence, a `source` claim that is never fetched, their `files` under `vendor/`, and a `licenseFile`). Unknown fields refuse.

## Bridge messages

Every message carries `mfw: 1`. The runtime ignores anything not from `window.parent`. Host to runtime: `init {protocol: 1, widgetId, runtime, alt, options, sources, theme}` (source bytes arrive as `ArrayBuffer`s; `options` is typed and defaulted); `theme {mode, tokens}` on mode change; `snapshot-request {requestId}` (no reply before `init`). Runtime to host: `ready` once; `status {state: loading|loaded|error, message? <=200}`; `snapshot {requestId, png}` (< 8 MiB data URL). Posting `resize` or any unlisted type is a violation the host drops. Copy the listener shape from `docs/runtimes/samples/`; authors cannot import the app's `bridge.ts`.

## Tokens a runtime may rely on

`theme.tokens` holds exactly the 52 names of `theme::TOKENS`: fonts `--m-font-{body,heading,mono,math}`; `--m-size-{base,small}`, `--m-scale`, `--m-leading`, `--m-measure`, `--m-figure-max`, `--m-space-1..6`, `--m-radius`; `--m-color-{bg,surface,text,muted,rule, link,accent,target,error,error-bg,warning}`; `--m-tooltip-{bg,fg,border}`, `--m-shadow`; plate `--m-figure-{bg,surface,ink,accent}`; palettes `--m-cat-1..8`, `--m-seq-1..5`, `--m-div-{low,mid,high}`. Draw content on `--m-figure-bg` with `--m-figure-ink`/`--m-figure-accent`/palettes. `--m-paper` is reader-only and never sent. Fonts do not cross origins.

## Scan rules

`runtimes::scan::scan` runs static checks; the CSP enforces at runtime. Errors refuse the package (poster-only, never approvable):

- `url-load`: loads a remote URL (...); runtimes must be self-contained. Absolute or `//` URLs in `src href srcset poster data action`, CSS `url()`/`@import`, or quoted JS strings.
- `meta-refresh`: uses `<meta http-equiv="refresh">`.
- `eval`: uses `eval()`, `new Function()`, or a string `setTimeout`/ `setInterval`.
- `module-import`: a module `import`/`export`; runtimes are classic scripts.
- `network-api`: `fetch`, `XMLHttpRequest`, `WebSocket`, `EventSource`.
- `worker`: `new Worker`, `importScripts`, shared/service workers.
- `storage`: `localStorage`, `sessionStorage`, `indexedDB`, `document.cookie`.
- `missing-ref`: references "...", which is no file in the package.

Warnings (export proceeds; approval shows them):

- `vendored-url`: the URL rules inside a vendored file.
- `unreferenced`: folded into the widget but nothing references it.
- `global-hook`: assigns a `window.__` hook.
- `size`: the package is over the 5 MiB budget.

Vendored files get only `vendored-url`. Metadata (`runtime.json`, `samples/**`, readmes, licences) is neither scanned nor `unreferenced`. The checks are regexes over `.html`/`.css`/`.js` (inline blocks included): matches inside comments flag, template literals and split strings slip through; the sandbox catches what they miss.

## Worked example: caption-overlay@1

`docs/runtimes/samples/caption-overlay@1/` shows an SVG with a text bar. `samples/photo.svg` exercises the required `image` role; `runtime.json` declares it primary with extensions `svg png jpg`, plus two options: `caption` (string, default `"An example overlay"`) and `position` (`top|bottom`, default `bottom`). `index.html` listens for `init`, turns `d.sources.image.bytes` into a blob URL, draws it on a canvas, paints the bar in `--m-figure-accent`, and answers `theme` and `snapshot-request`. Use it in a paper:

```latex
\interactiveruntime[runtime=caption-overlay@1, poster=figures/photo.png,
  alt={Harbour at dusk}, id=fig-harbour,
  sources={image=img/harbour.svg}, caption={Harbour at dusk}]{img/harbour.svg}
```

Copy the folder to `runtimes/caption-overlay@1/` in a project to try it. `bad-cdn@1` next to it is the negative: one CDN `<script src>`, which the scanner fails with exactly `url-load`.
