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

## Proposed posters

`poster=` is the document's own and always wins (D1). Without one, an approved runtime may propose the poster's pixels (D2): during the compile's poster pass the app folds the judged snapshot's files exactly as the export folds them and runs the runtime twice, once per colour mode. Both snapshots must be sane non-blank PNGs that differ (the runtime answers the theme); the light one becomes the poster. Anything else — a missing, invalid, unapproved, denied or licence-changed runtime, a blank or matching pair, a hang past the time limit — writes nothing, and the PDF shows the placeholder.

The proposal runs only while the runtime is approved (allowed at this digest, or an auto-covered content change): the poster cache keys it by the runtime digest, the ref, the bound sources and options, the poster theme and the renderer version, so any change names a poster that does not exist yet. The export's PDF picture is the mapped file; a map entry for an older state holds the placeholder with a `runtime-digest-changed` warning, never a refusal. The reader, the sanitizer and the manifest are untouched by proposals: a fallback widget is poster-only with its visible note, as before.

## Authoring flow: scaffold, validate, install, approve, export

An agent drafts the package through the MCP tools; the user carries it the rest of the way:

1. Scaffold: `runtime_scaffold` writes a draft to the user library (`maleficium-runtimes/<name>@1`), either caption-overlay-shaped (`runtime.json`, a classic `index.html`, a sample for the required role, `LICENSE`) or with `from: model@1` a fork of the built-in model viewer (its built entry plus its sources as reference). Refused when the draft folder exists and is not empty.
2. Validate: `runtime_validate` without `root_id` reads the library copy and reports the manifest, scan errors and warnings; with `root_id` it reads the project's installed copy instead. Read-only either way: it never approves, installs, or changes anything, and approval-shaped fields are refused.
3. Install: the user copies the library draft into a project as `runtimes/<ref>/` in the app. The agent cannot install.
4. Approve: the user allows the exact package content in the app (View > Widgets). Export mounts the runtime live only then and shows its poster otherwise.

The scaffold copies the built `bridge.js` into every draft (the plain draft's `index.html` loads it as a classic script): the runtime side of [Bridge messages](#bridge-messages), built from the app's `bridge.ts`, which authors never import.
