# Privacy policy

Maleficium runs entirely on your machine. There is no account, no server, and no cloud copy. This policy covers the app and its MCP server, which share one core and the same data handling.

## What stays on your machine

- Your projects stay in your own folders. The app never writes its build files, history, or logs into them.
- Build outputs, revision history, the trash, and the structured event log (newest 2000 lines, JSONL) live in app-local storage outside your projects.
- There is no telemetry and no auto-updater. The app never reports usage, contents, or diagnostics anywhere.

## Network

The only network traffic the app makes is the bundled Tectonic engine downloading TeX support files it does not have cached: on the first compile, and later only to fill cache gaps while the machine is online. After that everything works offline.

## The MCP server

- It speaks over stdio on your machine and does nothing until your agent starts it. It needs no network beyond the engine download above.
- It reads only the project roots you grant it, and only their text files.
- It writes only through its own tools — replace (previewed, one-step undo), trash with restore, new projects from templates — or to export destinations you choose outside the project. It never puts build files, history, or logs inside your projects.
- Every tool call is appended to the same event log with actor agent, so the log shows what the agent did.

Questions about this policy: open an issue on the Maleficium repository.
