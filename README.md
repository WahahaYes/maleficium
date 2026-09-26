# Maleficium

> **maleficium** _(n.)_: an act of evil sorcery. Also known as debugging LaTeX. This makes the ritual easier.

Maleficium is a LaTeX editor that runs entirely on your machine. Write on one side, read the PDF on the other, and click between them. It ships its own TeX engine, so there is nothing else to install.

![Maleficium with a LaTeX project open: file tree and outline, the editor, and the compiled PDF side by side](docs/screenshots/editor.png)

## Why Maleficium

**It runs locally.** There's no account, server, or cloud copy. Your projects stay in your own folders, and the app never writes its build files, history, or logs into them. The bundled Tectonic engine downloads its support files on the first compile; after that, everything works offline. That download is the only network traffic the app makes: no telemetry, no auto-updater.

**It's open source, so you can make it yours.** Maleficium is Apache 2.0 and built to be changed at the source: fork it and shape it to the way you work. Menus and the command palette are built from one command registry, so a new action shows up in both from a single entry. A template is just a folder, a color theme is a standard VS Code theme file, and the Rust core behind the editor is the same one its automation tools use. [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) gets you from clone to running app. No fork needed for the everyday cases: save any project as a template, or import a folder as one, and it joins the gallery beside the CC0 built-ins.

**It's built for working alongside AI agents.** Every action the app takes is written to a structured JSONL event log that an agent can read. When another process recompiles your document, the preview reloads on its own. A full agent interface, an MCP server that can compile, search, replace, and jump through SyncTeX, lives in the source tree today and will ship in the packaged app in a later release.

## Features

- **Compile with one key.** Ctrl+R compiles with live progress. A pre-compile check lists missing packages, fonts, and tools before the engine trips over them.
- **SyncTeX both ways.** Double-click a line to find it in the PDF; click the PDF to jump back to the source.
- **Find your way around.** File tree, document outline, go to definition for labels, citations, and macros, and a quick file finder (Ctrl+P).
- **Search and replace across the project.** Every replacement is previewed before it is applied, and the whole replace undoes in one step.
- **Revision history.** Every save keeps a revision you can restore (Ctrl+H).
- **Templates and themes.** Nine built-in starter templates, from articles and books to beamer slides and a CV, plus 18 bundled VS Code themes, dark and light.

<p>
  <img src="docs/screenshots/search.png" width="49%" alt="Project-wide search in the dark theme, with matches grouped by file">
  <img src="docs/screenshots/replace-light.png" width="49%" alt="A replace preview in a light theme, each match shown struck through beside its replacement">
</p>

## Install

Download from the [releases page](https://github.com/wahahayes/maleficium/releases). Linux x86_64 only for now.

Debian / Ubuntu:

```sh
sudo apt install ./Maleficium_0.1.0_amd64.deb
```

Any other distribution, with the AppImage:

```sh
chmod +x Maleficium_0.1.0_amd64.AppImage
./Maleficium_0.1.0_amd64.AppImage
```

If the AppImage fails with a FUSE error, run it with `APPIMAGE_EXTRACT_AND_RUN=1` set. To build it yourself, see [docs/BUILDING.md](docs/BUILDING.md).

## Getting started

On first launch Maleficium opens a short welcome project. After that:

1. **File > Open Project** (Ctrl+O) opens any folder of `.tex` files, or **File > New Project from Template** starts a new one.
2. Press **Ctrl+R** to compile. Maleficium finds the main file itself. To choose a different one, right-click a `.tex` file in the tree and pick **Set as Main File**.
3. Press **?** to see every keyboard shortcut.

## Known limits in 0.1.0

- Linux x86_64 only. macOS and Windows builds will come in a later release.
- The MCP server for AI agents is not in the packaged app yet; build from source to try it.
- No automatic updates. Check the releases page for new versions.
- 0.1.x makes no promise that settings or history carry over between versions.
- The engine is Tectonic only. Documents that need `biber` or shell escape depend on tools outside the app, and the pre-compile check flags them.

## Documentation

- [Building from source](docs/BUILDING.md), including how releases are cut
- [Contributing](docs/CONTRIBUTING.md): setup, checks, and house rules
- [Test harnesses](e2e/README.md)
- [Built-in templates](src-tauri/templates/README.md): origins and license

## Contributing and license

Contributions are welcome; see [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md). Release notes are in [CHANGELOG.md](CHANGELOG.md).

Licensed under Apache 2.0 ([LICENSE](LICENSE)). The built-in templates are CC0, so documents you make from them carry no obligations ([details](src-tauri/templates/README.md)). Bundled third-party themes and engine binaries are credited in [NOTICE](NOTICE).
