# Changelog

## Unreleased

### For AI agents

- **The MCP server ships in every package.** `maleficium --mcp` runs it from the app binary, so AI clients can point at the AppImage itself; Linux packages also install it as `maleficium-mcp`. The README's "Use with an AI agent" section has setup for Claude Code, Claude Desktop, VS Code, and opencode.
- The server now names itself `maleficium` with the app's version when a client connects.

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
