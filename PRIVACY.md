# Privacy policy

Maleficium runs entirely on your machine. There is no account, no server, and no cloud copy. This policy covers the app and its MCP server, which share one core and the same data handling.

## What stays on your machine

- Your projects stay in your own folders. The app never writes its build files, history, or logs into them. The one exception is the poster cache: when a paper uses interactive widgets without a poster of their own, a compile writes a `.maleficium/` folder (a README and rendered poster images) beside the main file. It is safe to delete and holds nothing but images and a lookup file for your own widgets.
- Build outputs, revision history, the trash, the structured event log (newest 2000 lines, JSONL), your widget approvals, and the browser-preview copy of a paper bundle live in app-local storage outside your projects.
- While the app runs it keeps one small file there, `maleficium-instances/<process id>.json`: the project folder it has open, the document's main file and the file in front. It is removed when the app closes, and ignored once it is a few minutes stale. The MCP server reads it so an agent can work in the project you already have open.
- There is no telemetry and no auto-updater. The app never reports usage, contents, or diagnostics anywhere.

## Network

The only network traffic the app makes is the bundled Tectonic engine downloading TeX support files it does not have cached: on the first compile, and later only to fill cache gaps while the machine is online. After that everything works offline.

## The MCP server

- It speaks over stdio on your machine and does nothing until your agent starts it. It needs no network beyond the engine download above.
- It reads only the project roots you grant it, and only their text files, plus which project and files your open app windows hold (the file above). It never opens, focuses, or changes anything in the app.
- It writes only through its own tools — replace (previewed, one-step undo), trash with restore, new projects from templates, installing a validated widget runtime draft as `runtimes/<name>@<major>/` — or to export destinations you choose outside the project. It never puts build files, history, or logs inside your projects, and it cannot approve a widget for you. A compile it runs writes the same `.maleficium/` poster cache the app does.
- Every tool call is appended to the same event log with actor agent, so the log shows what the agent did.

Questions about this policy: open an issue on the Maleficium repository.
