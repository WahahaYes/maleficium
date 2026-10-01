# Changelog

## Unreleased

### Templates

- **One look across the built-in templates.** The Resume and CV are redesigned around a shared style: Libertinus type, a violet accent, a two-tone slash that opens every heading, slanted skill chips, and a "Made with Maleficium" footer. Article, Assignment, Book, Letter, Report and the Welcome tour share a matching document style, and Presentation has a matching beamer theme. Journal Paper keeps plain IEEEtran formatting. Each folder carries its own copy of the style file, so a project stays self-contained.

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

### For contributors

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
