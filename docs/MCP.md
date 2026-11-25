# MCP server

Maleficium includes a local [MCP](https://modelcontextprotocol.io) server that speaks over stdio. It is the app binary with `--mcp`; Linux packages also install it as `maleficium-mcp`. It runs only when your agent starts it. [PRIVACY.md](../PRIVACY.md) covers what it reads, writes, and sends.

## The command

| Install  | Command                                                                       |
| -------- | ----------------------------------------------------------------------------- |
| `.deb`   | `/usr/bin/maleficium --mcp` (or `/usr/bin/maleficium-mcp`)                    |
| AppImage | `/path/to/Maleficium_<version>_amd64.AppImage --mcp`                          |
| macOS    | `/Applications/Maleficium.app/Contents/MacOS/maleficium --mcp`                |
| Windows  | `%LOCALAPPDATA%\Maleficium\Maleficium.exe --mcp` (the default install folder) |

For the AppImage, point at the file itself: its mount point changes on every run. The examples below use the `.deb` path.

## Clients

**Claude Code:**

```sh
claude mcp add maleficium -- /usr/bin/maleficium --mcp
```

**Claude Desktop** (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "maleficium": { "command": "/usr/bin/maleficium", "args": ["--mcp"] }
  }
}
```

**VS Code** (`.vscode/mcp.json`, or **MCP: Add Server** in the Command Palette):

```json
{
  "servers": {
    "maleficium": { "type": "stdio", "command": "/usr/bin/maleficium", "args": ["--mcp"] }
  }
}
```

**opencode** (`opencode.json`, or `opencode mcp add maleficium -- /usr/bin/maleficium --mcp`):

```json
{
  "mcp": {
    "servers": {
      "maleficium": { "type": "local", "command": ["/usr/bin/maleficium", "--mcp"] }
    }
  }
}
```

## What an agent can do

The server uses the same core as the editor: compile with structured diagnostics, outline, search, references, SyncTeX jumps, cross-file replace with preview and undo, export, and file operations, all scoped to the project folders the agent grants. Agents write text with their own file tools.

It sees which project your open app window holds, so an agent can join the folder you're in. It can ask the window to open another project, which happens only when you click Open. While the app is open, its preview reloads whenever the agent compiles.

## One-shot export command

For CI and headless use, `maleficium export` compiles a project and writes a paper bundle in one command, without opening a window or needing a display.

```sh
maleficium export <project-dir> --main <rel-tex> --out <dest> [--single-file] [--deny-warnings]
```

| Argument | Required | Description |
| --- | --- | --- |
| `<project-dir>` | yes | The project root |
| `--main <rel>` | yes | Main TeX file, relative to the project root |
| `--out <dest>` | yes | Where to write: a folder, or with `--single-file` a file. Outside the project |
| `--single-file` | no | Write a self-contained `index.html` instead of a folder bundle |
| `--deny-warnings` | no | Exit with code 3 if any export warnings are produced |

**Exit codes:**

- `0` — success
- `1` — compile failure (the log's last 15 lines, notes left out, on stderr)
- `2` — usage error
- `3` — `--deny-warnings` set and warnings were produced

**Example (folder bundle):**

```sh
maleficium export /path/to/project --main main.tex --out /path/to/bundle
# produces: /path/to/bundle/index.html, manifest.json, paper.pdf, theme/, widgets/
```

**Example (single-file bundle):**

```sh
maleficium export /path/to/project --main main.tex --out /path/to/bundle.html --single-file
# produces: /path/to/bundle.html (self-contained, with inlined manifest and PDF)
```

It uses the same compile and export path as the MCP `paper` and `export_write` actions, with the same trust rules: nothing is approved from the command line, so unapproved HTML widgets and custom runtimes export as posters. Each warning is one stderr line, `kind: message`.
