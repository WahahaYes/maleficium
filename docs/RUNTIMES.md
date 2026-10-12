# Custom widget runtimes

A custom runtime is a small self-contained page that draws one interactive widget inside an exported paper bundle. The author writes `\interactiveruntime[runtime=stl-viewer@1, ...]{models/part.stl}`. The PDF shows the widget's poster; the web bundle mounts the runtime live next to it.

Working samples live in `embed-runtime/samples/`. Read them alongside this guide.

## Folder layout

Export reads the project copy at `runtimes/<name>@<major>/`. Drafts live in the user library and are installed from there (see [Authoring flow](#authoring-flow-scaffold-validate-install-approve-export)).

```
runtimes/caption-overlay@1/
  runtime.json        required
  index.html          required entry; classic scripts and styles only
  app.js, style.css   your own code, referenced from index.html
  vendor/<lib>/...    third-party code plus its licence file
  samples/<file>      at least one file per required source role
  README.md, LICENSE  optional
```

The package holds regular files only, no symlinks. File-count and size limits are `MAX_FILES` and `MAX_BYTES` in `src-tauri/core/src/widget_approval.rs`.

Metadata is carried for review but never exported into the page: `runtime.json`, `samples/`, `README.md`, `LICENSE`, and each vendored `licenseFile`. Everything else is inlined into one document, `widgets/<id>/index.html`. That document runs in a `sandbox="allow-scripts"` frame with no network, no `eval`, no storage, no workers, and no module `import`.

## runtime.json

The manifest names the runtime, its licence, the source files it accepts, its options, and its capabilities. Unknown fields are refused. The parser and every limit (patterns, lengths, counts, the licence allowlist in `LICENSES`, reserved names and option keys) are in `src-tauri/core/src/runtimes.rs` (`parse_manifest`). The fields:

- `contract`: `1`.
- `name`: matches the folder before `@`. `version`: `MAJOR.MINOR.PATCH`, with the major matching the folder.
- `title`, `description`, `authors`, `license` (from the allowlist).
- `sources`: the roles the widget binds files to. Each role is `{required, extensions, primary?, maxBytes?, description?}`. Exactly one role is `primary: true`, and it must be required.
- `options`: an optional closed schema of string, number, integer and boolean properties, each with a default. The host sends them typed and defaulted.
- `capabilities`: `{webgl, wasm?}`. `wasm` defaults to false; see [Contract 2: WebAssembly](#contract-2-webassembly).
- `vendored`: each third-party library's name, version, allowlist licence, `source` (a claim, never fetched), its `files` under `vendor/`, and its `licenseFile`.

## Bridge messages

The only channel between the widget and its host is `postMessage`. Every message carries `mfw: 1`. Ignore anything not from `window.parent`.

Host to runtime:

- `init {protocol: 1, widgetId, runtime, alt, options, sources, theme}`. Each source arrives as bytes (`ArrayBuffer`) under its role name.
- `theme {mode, tokens}` when the colour mode changes.
- `snapshot-request {requestId}`. Never sent before `init`.

Runtime to host:

- `ready`, once.
- `status {state: loading|loaded|error, message?}`, message at most 200 characters.
- `snapshot {requestId, png}`, a PNG data URL under 8 MiB.

Any other type is dropped. Do not send `size`: the exporter adds a reporter to every widget document that posts its height whenever it changes.

The reference implementation is `embed-runtime/src/bridge.ts`. Authors cannot import it; the scaffold copies its built form, `bridge.js`, into every draft.

## Layout: width from the host, height from your content

The reader gives a widget its column's width (or the author's `width=` fraction of it, at least 20rem when the column allows). The frame then follows your content's height, held between 120 px and the larger of 960 px and twice the window; past that it scrolls. Until the first height report the frame keeps the poster's shape. In a poster render the frame is the poster box: fill it as you would at that width.

- **Size from width, never from height.** No `height: 100%`, `100vh` or `innerHeight`: the frame follows the document, so a height taken from the frame grows until the cap. Give media `width: 100%` and an `aspect-ratio`, with a px `max-height` if needed.
- **Mobile first, one breakpoint.** The frame's viewport is your width, so `@media (min-width: 560px)` works. Below it, one column: controls, media, readout. Above it, side by side.
- **Controls wrap, never clip.** Use `flex-wrap` on button groups and shorten long labels below the breakpoint.
- **Touch targets of 44 px** under `@media (pointer: coarse)`, sliders included.
- **Nothing only on hover.** What a tooltip shows also appears in a readout, and a pointer pick also works from the keyboard.
- **Don't trap the page's scroll.** Plots take `touch-action: pan-y`. A stage that needs every gesture (a 3D orbit) takes `touch-action: none` and stays at most 480 px tall.
- **Keep your height steady.** If a control changes how much text shows, reserve the tallest state's height.
- **Redraw on width changes only.** `resize` also fires when your own height changes the frame; compare the width first.

## Tokens a runtime may rely on

`theme.tokens` holds the CSS custom properties listed in `theme::TOKENS` (`src-tauri/core/src/theme.rs`): fonts, sizes, spacing, UI colours, and the figure tokens. Apply them on `:root`, as `applyTheme` in `bridge.ts` does.

Draw with the figure tokens:

- **Plate:** `--m-figure-bg` (backdrop), `--m-figure-surface`, `--m-figure-ink`, `--m-figure-accent`.
- **Palettes:** categorical `--m-cat-*`, sequential `--m-seq-*`, diverging `--m-div-{low,mid,high}`.

The plate is the widget's default backdrop. It is the page colour in the PDF poster, the reader's colour in the live page, or the author's `plate=` on that widget, which comes with an ink that reads on it. A runtime may offer its own background option; the plate is what it gets otherwise.

Fonts do not cross origins: the frame gets the font token's family names, not the font files.

## Scan rules

Before a package can be approved, `runtimes::scan::scan` (`src-tauri/core/src/runtimes/scan.rs`) checks its code statically. The sandbox's content security policy is the real boundary; the scan catches honest mistakes early. Each rule's id, message and hint are in that file.

Errors make the package invalid: it exports as its poster and cannot be approved. They cover:

- loading anything remote (absolute or `//` URLs in markup, CSS or quoted JS strings) or a `<meta>` refresh;
- dynamic code (`eval`, `new Function`, string timers) and module `import`/`export`;
- network, worker and storage APIs;
- WebAssembly without `capabilities.wasm`;
- a reference to a file not in the package.

Warnings let export proceed and are shown at approval: remote URLs inside vendored files, inlined files nothing references, `window.__` globals, and a package over the size budget.

Vendored files are checked only for remote URLs. Metadata is not scanned. The checks are regexes over `.html`, `.css` and `.js`, inline blocks included, so a match inside a comment is flagged and a split string slips through.

## Worked example: caption-overlay@1

`embed-runtime/samples/caption-overlay@1/` draws an image with a caption bar. It shows a manifest with one required source role and two options, an `index.html` that answers `init`, `theme` and `snapshot-request`, and a sample file. The scaffold starts every plain draft from it. To try it, copy the folder to `runtimes/caption-overlay@1/` in a project and write:

```latex
\interactiveruntime[runtime=caption-overlay@1, poster=figures/photo.png,
  alt={Harbour at dusk}, id=fig-harbour,
  sources={image=img/harbour.svg}, caption={Harbour at dusk}]{img/harbour.svg}
```

`bad-cdn@1` beside it is the negative example: one CDN `<script src>`, which fails the scan with `url-load`.

## Contract 2: WebAssembly

WebAssembly is opt-in. A runtime that compiles a module declares `"wasm": true` under `capabilities`. Undeclared use (the `WebAssembly` identifier, or a `.wasm` string or file reference) is a scan error. Vendored files are not checked for it; the policy catches what the scan misses.

Only a declaring runtime's document adds `'wasm-unsafe-eval'` to `script-src`. Every other directive is unchanged, and runtimes that do not declare never get it. Approving a runtime vouches for its WebAssembly as for the rest of its code. The bundle manifest records the declaration under the runtime's `capabilities`.

Without the token, browsers refuse to compile the module and the widget reports `status: error`. `embed-runtime/samples/wasm-sum@1/` is a declaring example: it sums a CSV with an embedded module.

## Proposed posters

An explicit `poster=` always wins. Without one, an approved runtime may propose its poster. During compile the app folds the runtime as an export would and renders it twice, once per colour mode, by sending `snapshot-request`. Both snapshots must be non-blank PNGs that differ (proof the runtime answers the theme); the light one becomes the poster. If the runtime is not approved, either snapshot fails, the pair matches, or it does not answer within the time limit, nothing is written and the PDF shows a placeholder.

Proposed posters are cached by the runtime's digest, its bound sources and options, and the theme. Any change to these renders a new poster on the next compile. If an export finds a poster made for an older runtime digest, it uses the placeholder and warns with `runtime-digest-changed`.

## Authoring flow: scaffold, validate, install, approve, export

An agent drafts and installs the package through the app's MCP tools. Approval is the user's alone.

1. **Scaffold.** `runtime_scaffold` writes a draft to the user library at `maleficium-runtimes/<name>@1`. By default it is a copy of caption-overlay plus `bridge.js`. With `from: model@1` it forks the built-in model viewer instead: a readable `viewer.js` you edit in place (no build step) over a vendored three.js, which validates cleanly untouched. It refuses a folder that already has files.
2. **Validate.** `runtime_validate` reports the manifest and scan findings for the library draft, or for a project's installed copy when given `root_id`. It changes nothing.
3. **Install.** `runtime_install` copies a draft that validates into a project as `runtimes/<ref>/`; copying the folder by hand works too. A different copy already there is kept unless `replace` is true. Installing is not approving: when the runtime is not approved, the result sets `ask_user`, and the agent asks the user to allow it in View > Widgets. A first install always needs this.
4. **Approve.** The user allows the exact package content in View > Widgets. Auto-approval covers only an update to an allowed runtime with the same licence and vendored libraries. Approval controls what runs inside the app: the in-app article and poster proposals.
5. **Export.** An export ships the runtime live whether or not it is approved, under the reader's sandbox and the widget's policy, as it does an html widget. Only a denied, invalid or missing runtime exports as its poster, with a warning.
