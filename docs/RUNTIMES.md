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

`contract` is exactly `1`. `name` matches the folder before `@` (`^[a-z][a-z0-9-]{1,39}$`, not a built-in or `m-*` name); `version` is `MAJOR.MINOR.PATCH` whose major matches the folder. `title` (1-80 chars), `description` (0-500), `authors` (1-16), `license` from the allowlist (MIT, BSD-2-Clause, BSD-3-Clause, Apache-2.0, ISC, Zlib, 0BSD, CC0-1.0, Unlicense). `sources` has 1-8 roles `{primary?: bool, required: bool, extensions: [...], maxBytes?: int, description?}` with exactly one `primary: true` that is also required. `options` is an optional closed object schema (<= 32 string/number/integer/ boolean properties with defaults, enums, minimum/maximum, maxLength). `capabilities` is `{webgl: bool, wasm?: bool}`: contract 1 packages omit `wasm` and parse with it off; see [Contract 2: WebAssembly](#contract-2-webassembly). `vendored` lists third-party libraries (name, version, allowlist licence, a `source` claim that is never fetched, their `files` under `vendor/`, and a `licenseFile`). Unknown fields refuse.

## Bridge messages

Every message carries `mfw: 1`. The runtime ignores anything not from `window.parent`. Host to runtime: `init {protocol: 1, widgetId, runtime, alt, options, sources, theme}` (source bytes arrive as `ArrayBuffer`s; `options` is typed and defaulted); `theme {mode, tokens}` on mode change; `snapshot-request {requestId}` (no reply before `init`). Runtime to host: `ready` once; `status {state: loading|loaded|error, message? <=200}`; `snapshot {requestId, png}` (< 8 MiB data URL); `size {height}` (CSS px). You never send `size` yourself: the exporter puts a reporter at the head of every widget document that posts the document's height whenever it changes. Posting any unlisted type is a violation the host drops. Copy the listener shape from `docs/runtimes/samples/`; authors cannot import the app's `bridge.ts`.

## Layout: width from the host, height from your content

The reader gives a widget the width of its column (or the author's `width=` as a fraction of it, never narrower than 20rem while the column allows) and sizes the frame to the widget's content height. Until the first report the frame keeps the poster's shape; the host holds the height between 120 px and the larger of 960 px and twice the window, past which the frame scrolls. A poster render is the exception: there the frame is the poster box, and your document should fill it as it would at that width.

- **Size from width, never from height.** No `height: 100%`, `100vh` or `innerHeight` on the document: the frame follows the document, so a height taken from the frame grows until the cap. Give media (plot, canvas, image, video) `width: 100%` and an `aspect-ratio`, with a `max-height` in px when a tall shape would get silly. `vh` inside the frame is the frame's own height; do not use it.
- **Mobile first, one breakpoint.** The frame's viewport is your width, so `@media (min-width: 560px)` works. Below it: one column, in reading order controls, then the media, then the readout. Above it, side by side.
- **Controls wrap, never clip.** `flex-wrap` button groups; drop long label suffixes below the breakpoint.
- **Touch targets of 44 px** under `@media (pointer: coarse)`, sliders included.
- **Nothing only on hover.** What a tooltip shows is also in a readout, and a pointer pick also works from the keyboard (a focusable plot that takes the arrow keys).
- **Don't trap the page's scroll.** Plots take `touch-action: pan-y` (a vertical swipe scrolls the article, a horizontal drag picks); a stage that needs every gesture (3D orbit) takes `touch-action: none` and stays at most 480 px tall so the page can be scrolled past it.
- **Keep your height steady under interaction.** When a control changes how much text shows, reserve the tallest state's height so the frame does not jump under a finger.
- **Redraw on a width change only.** The window's `resize` event also fires when your own height changes the frame; compare the width before redrawing.

## Tokens a runtime may rely on

`theme.tokens` holds exactly the 52 names of `theme::TOKENS`: fonts `--m-font-{body,heading,mono,math}`; `--m-size-{base,small}`, `--m-scale`, `--m-leading`, `--m-measure`, `--m-figure-max`, `--m-space-1..6`, `--m-radius`; `--m-color-{bg,surface,text,muted,rule, link,accent,target,error,error-bg,warning}`; `--m-tooltip-{bg,fg,border}`, `--m-shadow`; plate `--m-figure-{bg,surface,ink,accent}`; palettes `--m-cat-1..8`, `--m-seq-1..5`, `--m-div-{low,mid,high}`. Draw content on `--m-figure-bg` with `--m-figure-ink`/`--m-figure-accent`/palettes. The plate is the widget's default backdrop: the paper's page colour in the PDF poster (white unless the document sets `\pagecolor`), the reader's in the live page, or the author's `plate=` on that one widget, which holds in both modes and comes with the ink that reads on it. Paint on it unless your own option chooses otherwise (the built-in model viewer's `background=`, a chart spec's own `background`): the choice is the runtime's, the plate is what it gets by default. `--m-paper` is reader-only and never sent. Fonts do not cross origins.

## Scan rules

`runtimes::scan::scan` runs static checks; the CSP enforces at runtime. Errors refuse the package (poster-only, never approvable):

- `url-load`: loads a remote URL (...); runtimes must be self-contained. Absolute or `//` URLs in `src href srcset poster data action`, CSS `url()`/`@import`, or quoted JS strings.
- `meta-refresh`: uses `<meta http-equiv="refresh">`.
- `eval`: uses `eval()`, `new Function()`, or a string `setTimeout`/ `setInterval`.
- `module-import`: a module `import`/`export`; runtimes are classic scripts.
- `network-api`: `fetch`, `XMLHttpRequest`, `WebSocket`, `EventSource`.
- `worker`: `new Worker`, `importScripts`, shared/service workers.
- `storage`: `localStorage`, `sessionStorage`, `indexedDB`, `document.cookie`.
- `wasm`: `WebAssembly` use, a `.wasm` string, or a `.wasm` file reference without `capabilities.wasm` (see below).
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

## Contract 2: WebAssembly

Contract 1 deliberately has no WASM. A runtime that compiles WebAssembly declares `"wasm": true` under `capabilities` (omitted means false, so every contract 1 package parses unchanged). The scanner refuses undeclared use as a `wasm` error — the `WebAssembly` identifier, a `.wasm` string in script, or a `.wasm` file reference from markup, style or script — so an undeclared package is invalid: poster-only, never approvable, like `bad-cdn@1`. Vendored files are exempt from the content rules (as with `eval` and the rest); the sandbox catches what the scan misses.

A declaring runtime's folded document alone carries `'wasm-unsafe-eval'` in `script-src` (`script-src 'unsafe-inline' 'wasm-unsafe-eval'`); every other directive is the contract-1 policy, and non-declaring runtimes keep it byte for byte. Built-ins and html widgets never declare, so they never carry the token. The manifest's `runtimes` entry records the declaration (`"capabilities": {"webgl": ..., "wasm": ...}`), and the approval verdict and store are unchanged: allowing a runtime vouches for its WASM module as for the rest of its code.

Posters of declaring runtimes render under the widened policy (renderer version 2), so the proposed-poster cache key moves for every custom widget: the next compile rerenders them. `docs/runtimes/samples/wasm-sum@1/` is the declaring fixture (a CSV summed by an embedded add module); the non-declaring samples are the control.

Engine gating (reader cells, October 2026, `e2e/wasm-cells.mjs` over the proof below): the token is what lets the module compile. With it the fixture reaches live in Chromium, Firefox and WebKit; with the token stripped from both layers the widget posts `status: error` in all three — each engine gates `WebAssembly.instantiate` on the token. (The "WebKitGTK 2.52 does not gate" note predates upstream gating; the measured WebKit build 2359 gates.) The non-declaring control reaches live with a token-free policy in all three engines.

## Proposed posters

`poster=` is the document's own and always wins (D1). Without one, an approved runtime may propose the poster's pixels (D2): during the compile's poster pass the app folds the judged snapshot's files exactly as the export folds them and runs the runtime twice, once per colour mode. Both snapshots must be sane non-blank PNGs that differ (the runtime answers the theme); the light one becomes the poster. Anything else — a missing, invalid, unapproved, denied or licence-changed runtime, a blank or matching pair, a hang past the time limit — writes nothing, and the PDF shows the placeholder.

The proposal runs only while the runtime is approved (allowed at this digest, or an auto-covered content change): the poster cache keys it by the runtime digest, the ref, the bound sources and options, the poster theme and the renderer version, so any change names a poster that does not exist yet. The export's PDF picture is the mapped file; a map entry for an older state holds the placeholder with a `runtime-digest-changed` warning, never a refusal. The reader, the sanitizer and the manifest are untouched by proposals: a fallback widget is poster-only with its visible note, as before.

## Authoring flow: scaffold, validate, install, approve, export

An agent drafts and installs the package through the MCP tools; approval is the user's alone:

1. Scaffold: `runtime_scaffold` writes a draft to the user library (`maleficium-runtimes/<name>@1`), either caption-overlay-shaped (`runtime.json`, a classic `index.html`, a sample for the required role, `LICENSE`) or with `from: model@1` a fork of the built-in model viewer: `viewer.js` is the viewer as a readable classic script you edit in place (no build step), over three.js vendored at `vendor/three/` (declared in `vendored`, so the scan holds it only to `vendored-url`) and `bridge.js`. Untouched, a fork validates with no errors or warnings. Refused when the draft folder exists and is not empty.
2. Validate: `runtime_validate` without `root_id` reads the library copy and reports the manifest, scan errors and warnings; with `root_id` it reads the project's installed copy instead. Read-only either way: it never approves, installs, or changes anything, and approval-shaped fields are refused.
3. Install: `runtime_install` copies the library draft into a granted project as `runtimes/<ref>/` (or the user copies the folder by hand). Only a draft that validates installs; a different copy already in the project stays unless `replace` is true, and the same files report `unchanged`. Copying reviewed files is not an approval. The result says whether the runtime runs now (`approved`) and, when it does not, sets `ask_user` with a `hint` the agent acts on: ask the user to allow it in View > Widgets. Auto-approval covers only an update to a runtime the user already allowed with the same licence and vendored libraries, never a first install, so a new runtime always asks.
4. Approve: the user allows the exact package content in the app (View > Widgets). Approval decides what runs inside the app (the in-app article and the poster proposal); an export ships the runtime live whether or not it is approved, as it does an html widget, under the reader's sandbox and the widget's strict CSP. Only a denied, invalid or missing runtime exports as its poster, with a warning.

The scaffold copies the built `bridge.js` into every draft (the plain draft's `index.html` loads it as a classic script): the runtime side of [Bridge messages](#bridge-messages), built from the app's `bridge.ts`, which authors never import.
