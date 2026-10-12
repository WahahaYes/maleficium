# Privacy policy

Maleficium runs entirely on your machine. There is no account, no server, and no cloud copy. This policy covers the app and its MCP server, which share one core and handle data the same way.

## What stays on your machine

- Your projects stay in your own folders. The app never writes build files, history, or logs into them. The one exception is the poster cache: when a paper has interactive widgets without a poster of their own, a compile writes a `.maleficium/` folder of rendered posters beside the main file. It is safe to delete.
- Everything else the app keeps lives in your system's app-data and cache folders, outside your projects: build outputs, revision history, the trash, the structured event log (rotated, so only recent lines remain), widget approvals, your own templates and widget runtimes, and the browser-preview copy of a paper bundle.
- While the app runs it keeps one small file in app data, `maleficium-instances/<process id>.json`, naming its open project folder, main file, and the file in front. It is removed on exit and ignored once the app stops updating it. The MCP server reads it so an agent can join the project you have open.
- There is no telemetry and no auto-updater. The app never reports usage, contents, or diagnostics anywhere.

## Network

The only network traffic the app makes is the bundled Tectonic engine downloading TeX support files it has not cached: on the first compile, and later only to fill cache gaps while the machine is online. After that everything works offline.

Preview in Browser and exported paper bundles open in a browser, outside the app, where a widget can load from the `https` origins it declares ([details](docs/VIDEO-EMBEDS.md)).

## The MCP server

- It speaks over stdio, runs only when your agent starts it, and needs no network beyond the engine download.
- It works only inside the project folders its agent grants for the session, and refuses paths that escape them. It also reads which project and files your open app windows hold (the file above). It can ask a window to open a project folder, but nothing opens unless you click Open.
- Inside a project it writes only through its own tools: replace (previewed, with one-step undo), trash with restore, and installing a validated widget runtime as `runtimes/<name>@<major>/`. A compile it runs writes the same `.maleficium/` poster cache the app does. Its other writes go outside your projects: new projects from templates, exports to a destination outside the project, and runtime drafts in the app's runtime library.
- It cannot approve a widget for you.
- Every tool call is appended to the same event log, marked as the agent's, so the log shows what the agent did.

Questions about this policy: open an issue on the Maleficium repository.
