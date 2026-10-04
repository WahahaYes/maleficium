# Changelog

## Unreleased

### Changes

- **The interactive package comes with the template.** Tools > Install Interactive Package is gone. Every project made from a template already contains `maleficium-interactive.sty` at its root; after that it is an ordinary project file and the project folder wins, as with any package. A `.sty` or `.cls` the TeX bundle does not have is an error that names the file and says to add it to your project folder next to your main file. Nothing is downloaded or installed by the app.
- **Interactive posters are cached in the project.** A model, chart or HTML widget with no `poster=` gets a poster Maleficium renders itself, saved in `.maleficium/posters/` beside the main file when you compile. This is the one place a compile writes into your project. The folder has a README, is safe to delete and may be committed, so the paper compiles with its posters without Maleficium. A widget new in a compile shows a placeholder until the next compile. Source zips and bundle exports include the cache. `\interactivemodel` takes `camera=`, `size=` and `background=`, and `\interactivechart` takes `scale=`, to shape the poster.
- **Your own HTML widgets need approval.** View > Widgets lists each widget with its status, a diff of what changed since you approved it, Approve and Revoke, and an auto-approval setting that is off by default. An unapproved widget is never rendered, and an agent over MCP can read the status but cannot approve.
- **Preview in Browser and a reflowed reader.** File > Preview in Browser exports the paper as one file and opens it. The bundle's `index.html` is now the paper itself, reflowed from your LaTeX with each widget mounted in place, and `paper.pdf` stays in the bundle as the version of record and a download, not embedded. Anything the conversion could not read, join or find (a macro, a widget, a figure) is listed in the export's warnings. An undefined macro gets one warning per macro name (with its spots and the project-folder hint for its package) instead of one bare count; a package the log says needs shell escape, or a file the bundle lacks, is named as such.
- **Html widgets can declare origins.** `framedomains=` and `resourcedomains=` (or `widget.json`) name the `https` origins one widget may frame or load, for example a YouTube or Vimeo player; see docs/VIDEO-EMBEDS.md. The origins are part of the approval.
- **One QR code per paper, in the page footer.** `maleficium-interactive.sty` drew a 2cm QR code under every widget when `bundleurl` was set. It now draws one 1.5cm QR code with the URL in the footer of the first page, and each widget keeps a one-line "Interactive version" mark that links to the bundle URL. Floats no longer grow by 2cm and widget rects are unchanged. `\maleficiumsetup{qr=none}` turns the footer QR off.

### Fixes

- **A finished compile no longer reads as "compile worker lost".** Polling a job after it finished returned the result once, then failed with "compile worker lost" and then reported it as running again. Anything that polled a job twice (an agent plus a live view, or two views) saw a failed or stuck compile. Every poll of a finished job now returns the same final result.
- **A label's snippet lands on its figure.** `snippet` with a `label` (or a line such as `\label` or `\end{figure}` that has no box of its own) came back as the whole page body on the page where the float's text sits, so an agent asking about a figure got the wrong page. It now uses the nearest earlier line that has a placement of its own, which puts the snippet on the figure.

### For AI agents

- **Hosts that support MCP Apps show compiles live.** `compile_run` now names a compile dashboard View (`ui://maleficium/compile/v1`): status, the phase reached and files downloaded, the log tail, offline-readiness and missing-dependency findings, the failing source lines, and a Cancel button. It polls only while visible and stops when the compile finishes. Other hosts get the same results as text. `compile_run` also returns the `main_rel` it compiles, and `compile_poll` reports `progress` (the phase, plus files downloaded and failed).

- **Hosts that support MCP Apps show the snippet inline.** The `snippet` tool now names a small View (`ui://maleficium/snippet/v1`): the rendered region of the PDF beside the source lines, with previous/next page, a whole-page toggle, and a refresh when a compile replaces the PDF. It follows the host's light or dark theme and never inverts the page. Hosts without Apps support get the same text and, with `with_image`, the same image as before. A new `snippet_render` tool, hidden from the model, lets the View re-render without a model turn.

- **Every MCP tool declares what it does.** All 32 tools now carry explicit read-only, destructive, idempotent and open-world hints, so hosts and directories can tell a safe read from a write without guessing. A regression test fails if a tool is added without them, and each tool has a handler test. `PRIVACY.md` states that the app and server work locally, and the README links it.

### Templates

- **One look across the built-in templates.** The Resume and CV are redesigned around a shared style: Libertinus type, a violet accent, a two-tone slash that opens every heading, slanted skill chips, and a "Made with Maleficium" footer. Article, Assignment, Book, Letter, Report and the Welcome tour share a matching document style, and Slides has a matching style. Journal Paper keeps plain IEEEtran formatting. Every template ends each page with a small "Made with Maleficium" mark that links to the project; deleting one `\usepackage{maleficium-footer}` line removes it. Each folder carries its own copy of the style files, so a project stays self-contained.

### For contributors

- **The TeX engine is built from source.** Compiling no longer runs a downloaded Tectonic binary. A new crate, `src-tauri/engine`, builds `maleficium-engine` (Tectonic 0.17.0 through its library API, with the same arguments and status lines) and still runs as a killable child of the app and the MCP server. It also links the latexml converter, as groundwork for a reflowed reader. Build it with `sh scripts/build-engine.sh` before `npm run build` or `tauri build`; CI builds it on all four targets, and the old sidecar download script is gone.
- **The engine converts a paper to HTML with no TeX installed.** `maleficium-engine convert` resolves every file latexml asks for from the pinned Tectonic bundle (through a built-in `kpsewhich` mode, also reachable as `maleficium-engine kpsewhich`), generates the TeX Live 2022 kernel dumps at build time (`maleficium-engine dump`, run by `scripts/build-engine.sh` and cached in CI), and writes its stylesheets beside `--out` and never beside the source. It reports a failed post-processing step instead of returning raw XML, and tags each figure with `data-graphic`. The `maleficium-interactive` widgets convert to placeholders that join to the `.mfw` sidecar by order, and `reflow::figures` makes the article's figures self-contained. These are the pieces of the reflowed reader; nothing in the app calls them yet.
- **Sidecar downloads retry.** A single 504 from GitHub no longer fails a CI job: the download is retried four times, and the sha256 pin is still checked.

## 0.3.0 - 2026-09-30

### Fixes

- **Font-tracing noise no longer buries real warnings.** Compiling under XeTeX with font tracing on (for example through libertine.sty) reported dozens of fragment warnings such as `Requested font "nxlmi7" at 7.3pt` and `-> nxlmi7`, one per trace line. Those lines are now recognized as tracing noise and dropped, so the diagnostics list shows the warnings that matter.
- **A flaky macOS package step is hardened.** The Intel dmg bundle intermittently failed inside the bundler after a clean compile, stalling the release: hdiutil's create and detach steps race Spotlight indexing and report "Resource busy", and the bundler swallows the script's output so the log said nothing. `package.sh` now takes Spotlight indexing off CI runners before packaging, and if the bundle still fails it re-runs the bundle step verbose so the log names the failing call.
- **Saving never overwrites an agent's edit.** When another program (such as an AI agent) wrote a file you had open, the app could save your copy over it in the moment before it noticed the change: on autosave, switching or closing a file, or compiling. Every save now checks the file on disk first, and if it changed, you get the usual choice to reload or keep your edits instead.
- **An agent's first compile shows in the preview.** The preview only picked up outside compiles after the app had shown a PDF itself, so a project an agent compiled before you ever did stayed blank. It now shows the PDF as soon as one appears.
- **Ctrl+S says when it can't save.** A manual save refused because the file changed on disk, or one that failed, did nothing visible. It now says so in the status bar and the log.
- **Jumps into another file open that file.** Go to definition, a search or replace hit, and Ctrl+Tab stayed in the file you were in when the target was in a different one, and jumped to a line there instead. They open the target file again.
- **Go to Line works from the editor.** Ctrl+G also opened a stray search panel that took the keyboard, so the line number you typed went nowhere. Now only Go to Line opens, and it centers the line. F3 and Shift+F3 open the app's find bar.

### For AI agents

- **Agents can see how the PDF looks.** The new `snippet` tool finds where a source line, a label, or a page landed in the compiled PDF and returns that page, the region, and the source lines around it. With `with_image: true` it also returns the region as a PNG, so an agent can check a figure's size and placement or a table that overflows. Images are off by default because they cost model context.

- **Agent actions and yours share one event log.** The app and the MCP server now write to the same `events.jsonl`, each line tagged with who did it (`user`, `agent` or `system`), and every MCP tool call is recorded as `mcp.call`. The log is trimmed by rotation instead of being emptied at each launch, so an agent can read what happened before you opened the app.

### For contributors

- **The app's logic lives in the Rust core.** Compiling, saving and trash, main-file detection, the event log, and the file watcher moved into `maleficium-core`, behind one typed operation contract that the app and the MCP server both call. The frontend reaches Tauri only through `*.tauri.ts` adapters, and an eslint rule enforces it.
- **`App.tsx` is smaller.** Pane layout, file switching, saving, the main file, project search and replace, and the window keymap moved into hooks, and the editor, preview and side column into components.
- **The stills harness is Python.** `e2e/stills-run.sh` is now `e2e/stills-run.py`, with the same env vars, states and log checks.
- **A harness records showcase videos.** `e2e/showreel/` drives an agent through the app and cuts the take; the README embeds the result.
- **The playground is generated, not tracked.** `sh scripts/playground.sh` fills an ignored `playground/` with one project per built-in template, the `simple` test fixture (now at `e2e/fixtures/simple`), and the vendored papers.
- **Real papers as fixtures, and a smoke that scores them.** `e2e/fixtures/vendored/` holds real, permissively licensed papers with their provenance. `python3 e2e/papers-run.py` compiles each over the MCP server, lists every diagnostic, and fails only when a paper that should compile error-free stops doing so, or when one that should not starts to.
- **One MCP test client.** The e2e harnesses share `e2e/mcp_client.py` instead of five copies.
- **Building from source has its own section in the README.**

## 0.2.1 - 2026-09-27

### Fixes

- **LaTeX commands are readable in dark themes.** The editor drew commands like `\documentclass` and `\begin` in a dim purple that was hard to read on dark backgrounds. Syntax colors now come from the active theme, and each is adjusted until it meets WCAG AA contrast against the editor background, in every bundled theme.
- **A missing `biber` is named when a compile fails.** A document using biblatex's default biber backend failed with "bundled tectonic failed: No such file or directory", which read as if the app's own engine were missing. It now says biber is not installed and how to switch to `backend=bibtex`, which needs nothing outside the app.

### For contributors

- **Commit hooks fix formatting instead of only reporting it.** `cargo fmt`, `prettier` and `eslint --fix` rewrite the staged files; re-stage them and commit again. The Rust format check now covers every workspace crate, not just the app crate, in the hooks, `npm run check` and CI.

## 0.2.0 - 2026-09-27

Maleficium now builds for macOS and Windows as previews, and its MCP server ships inside the app so AI agents can write and compile LaTeX through it.

### New platforms (preview)

- **macOS** (Apple silicon and Intel, one `.dmg` each) and **Windows** (x86_64, `setup.exe` installer). Both are unsigned: macOS asks you to click **Open Anyway** once, and Windows SmartScreen asks you to confirm. The README has the steps.
- These builds pass automated checks on each platform (install, launch, compile, and the MCP server) but have not yet been tried by a person on a real Mac or Windows PC. Please report anything that goes wrong.

### For AI agents

- **The MCP server ships in every package.** `maleficium --mcp` runs it from the app binary, so AI clients can point at the AppImage itself; Linux packages also install it as `maleficium-mcp`. The README's "Use with an AI agent" section has setup for Claude Code, Claude Desktop, VS Code, and opencode.
- The server now names itself `maleficium` with the app's version when a client connects.
- `diagnostics` now reports undefined references and citations and duplicate labels, on the line that uses or defines them. Before, a document that compiled with `??` in it read as clean.
- `compile_poll` takes `wait_ms` to wait for a running compile instead of being called in a loop.
- A delete without its confirm now says which path to pass back, rather than reporting a mismatch.

### Fixes

- **The log lists undefined references, undefined citations, and duplicate labels after every compile**, each a click away from its line. Before, a document that compiled with `??` in it showed no problems.
- **Compiling no longer fails on a font the engine has not downloaded yet.** Once the engine's cache was filled, a document that first needed another Latin Modern face (a new size or italic, for example) failed with "not loadable" instead of fetching it.
- **Compiling fetches missing font metrics** instead of failing when the engine's cache lacks them.
- The status bar shows the compiled main file instead of the path of the cached PDF.
- Files whose names contain `__` restore correctly from the trash.
- The app and the MCP server now agree on a project's identity and trash, so a delete made by an agent can be undone in the app and the other way round.

### Known limits

- macOS and Windows builds are unsigned previews, less tested than Linux.
- Tectonic is the only engine. Documents that need `biber` or shell escape rely on tools outside the app; the pre-compile check flags them.
- 0.x makes no promise that settings or history carry over between versions.

## 0.1.1 - 2026-09-26

Bug-fix release. If you installed 0.1.0, please update: compiling did not work in the 0.1.0 packages.

### Fixes

- **Compiling works in the installed app.** The 0.1.0 `.deb` and AppImage could not find their bundled TeX engine, so every compile failed with "bundled maleficium-tectonic sidecar missing". SyncTeX was affected the same way.
- **The main file follows the project you open.** Opening a project while another was open (for example, creating one from a template while the welcome project was showing) left the previous project's main file in the editor header.
- **The AppImage no longer opens a blank window on newer distributions** such as Ubuntu 26.04. It carried its own copies of graphics libraries that clash with the system's newer drivers; it now uses the system's.

### Known limits

- Linux x86_64 only. macOS and Windows builds will come in a later release.
- Tectonic is the only engine. Documents that need `biber` or shell escape rely on tools outside the app; the pre-compile check flags them.
- 0.1.x makes no promise that settings or history carry over between versions.

## 0.1.0 - 2026-09-26

First public release, for Linux x86_64 (`.deb` and AppImage).

### Writing and compiling

- Editor and PDF preview side by side, with SyncTeX both ways: double-click a line (or Ctrl+Alt+J) to find it in the PDF, click the PDF to jump back to the source.
- Bundled Tectonic engine, so no TeX install is needed. The first compile downloads its support files; after that, compiles work offline. **Make Available Offline** fetches everything a project needs up front.
- Ctrl+R compiles with the current phase and download count shown live in the status bar, and a running compile can be cancelled. **Auto-Compile on Save** is a toggle.
- Maleficium finds the main file itself; right-click any `.tex` file and pick **Set as Main File** to override it.
- A pre-compile check lists missing packages, fonts, `biber`, and shell escape before the engine fails on them.
- The preview reloads by itself when another process recompiles the PDF, and has zoom and fit-to-width (Ctrl+= / Ctrl+- / Ctrl+0).
- **Export PDF** and **Export Project as Zip** from the File menu.

### Finding your way around

- File tree, document outline with a symbol filter, and go to definition (F12 or Ctrl+click) with hover for labels, citations, macros, and `\input` files.
- Go to File (Ctrl+P), Command Palette (Ctrl+Shift+P), and Go to Line (Ctrl+G).
- Find in file (Ctrl+F) and across the project (Ctrl+Shift+F), including unsaved changes.
- Replace across the project: every change is previewed first, and the whole replace undoes in one step.

### Your files stay yours

- Every save keeps a revision you can restore (Ctrl+H). History, build files, and logs are stored outside your project folder.
- Edits made to open files by other programs are detected; Maleficium holds the save instead of overwriting them.
- No account, telemetry, or auto-updater. The engine's first-compile download is the only network traffic.

### Projects and appearance

- Nine starter templates (CC0), from articles and books to beamer slides and a CV, plus a welcome tour on first launch.
- Save any project as a template, or import a folder as one.
- 18 bundled VS Code color themes, dark and light, with a searchable picker. Dark themes dim the PDF to match.
- Open Recent, with Clear Recents.

### For AI agents

- Every action the app takes is written to a structured JSONL event log that agents can read.
- An MCP server (compile, search, replace, SyncTeX). The 0.1.0 packages already contained it as `maleficium-mcp`, though these notes said otherwise at the time.

### Known limits

- Linux x86_64 only. macOS and Windows builds will come in a later release.
- Tectonic is the only engine. Documents that need `biber` or shell escape rely on tools outside the app; the pre-compile check flags them.
- 0.1.x makes no promise that settings or history carry over between versions.
