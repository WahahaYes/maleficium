# e2e — end-to-end harnesses

These scripts check the built app and its automation sidecar from the outside. They are test code and never ship in the app bundle. Run every command from the repo root.

| Harness | Needs a window? | What it proves |
| --- | --- | --- |
| `project-footprint.sh` | no | The app never writes into a user's project |
| `history-surface.sh` | no | The revision history surface is wired up and restores exact bytes |
| `search-run.sh` | no | Project search, structure tools, and replace over the sidecar |
| `driver-run.sh` | no | Compile, SyncTeX, and file ops over the sidecar, plus heavy-document budgets |
| `interactive-run.sh` | no | A project holding the interactive package compiles, and the widget list, sidecar and annotations are right |
| `export-run.sh` | no | Paper bundle export in every profile, preview in browser, declared origins, and the reader in Chromium, Firefox and WebKit |
| `convert-run.sh` | no | `maleficium-engine convert` with the host TeX hidden (bwrap, Linux): structure counts, figure paths, an undefined macro still yields the article, nothing written beside the source |
| `playground-run.sh` | no | The showcase paper compiles with every widget macro |
| `approval-run.sh` | no | Html widget approval over the sidecar: nothing can approve over MCP, and the app-side store is what counts |
| `posters-cache-run.sh` | Xvfb (own display) | The `.maleficium/posters` cache: two-pass compile, regeneration, cleanup, source zips and bundle export |
| `reader-run.mjs` | no | The reader page in headless Chromium, Firefox and WebKit (run by `export-run.sh`) |
| `widget-runtimes/run.mjs` | no | The model and video runtimes in a sandboxed frame in headless Firefox |
| `poster-run.sh` | Xvfb (own display) | Widget posters render headlessly from their camera, size, background and scale, and a hanging runtime is killed at its time limit |
| `stills-run.py` | Xvfb | Screenshots of each app state, and the app's own event log |
| `papers-run.py` | no | How many vendored real papers compile error-free, and what blocks the rest |
| `package-smoke.py` | Linux: Xvfb in Docker | This host's packages install, compile with the bundled engine, and launch into the welcome project |
| `agent-run.py` | no | A real LLM agent can do LaTeX tasks through the MCP server (manual; spends model credits) |
| `codrive-run.py` | Xvfb | An agent edits and compiles over MCP while the app is open: the preview follows, and no buffer or file is lost to the other side (manual; spends model credits) |
| `showreel/` | Xvfb | An agent writes a document live in the open app while a director follows it for the camera, judged beat by beat (manual; spends model credits) |

## Running in isolation

Run harnesses through `worktree-run.sh`, which checks out a pinned commit into a separate worktree so edits in your checkout cannot disturb a run:

```sh
./e2e/worktree-run.sh [<ref>] -- ./e2e/stills-run.py
```

- It carries your uncommitted `e2e/` changes into the worktree, so you can test a harness before committing it.
- It reuses one worktree under `/var/tmp` and its own Cargo target dir, so warm runs rebuild nothing and never touch your dev build.
- `node_modules` is symlinked in; vite gets its own dep cache.
- If another run holds the worktree, it falls back to a throwaway one (kept on failure for inspection).

GUI harnesses serve on port 1420. Set `STILLS_PORT` to move a stills run while a dev app holds that port.

## The harnesses

### project-footprint.sh

A static audit. Copies `e2e/fixtures/simple/` into a scratch git repo, replays each of the app's path derivations (trash, history, outputs, event log, main-file association) in bash, and asserts `git status` stays clean. It also pins the security config: no `$HOME` in capabilities, CSP enforced, and the opener plugin absent (lockfiles included).

### history-surface.sh

A static audit of the revision history: both entry points reach one command, the UI speaks in user terms, and a restore through the store's blob layout returns the exact original bytes.

### search-run.sh

Drives the `maleficium-mcp` sidecar over JSON-RPC against a scratch copy of `e2e/fixtures/simple/`, with no compile and no network. Covers search (literal, regex, invalid regex, result cap), main-document ranking, a file written mid-run, `find_files`, the structure tools, go to definition, and replace (preview writes nothing, apply once by token, undo restores exact bytes, a stale plan is refused). Build the sidecar first:

```sh
cargo build --manifest-path src-tauri/Cargo.toml --bin maleficium-mcp
./e2e/search-run.sh
```

### driver-run.sh

Drives the sidecar through grant → compile → poll → SyncTeX → delete → undo, then checks nothing landed in the project. A second half runs heavy documents: opening a 1000-file folder, cancelling a 3000-page compile, and render budgets (PDF load, page render, peak memory) measured with the same pdf.js build the preview uses. Exits non-zero when a check or budget misses.

- Heavy fixtures are generated by `src/test/make-fixture.ts` into the OS temp dir on first run and reused. `DRIVER_FIXTURES` relocates them.
- `DRIVER_LOG` sets the JSONL run log, `WARM_ONLY=1` stops after the first compile, `POLL_ROUNDS` bounds polling, and `MCP_ROOT_OVERRIDE` drives an existing folder in place.
- The render budgets need `@napi-rs/canvas`, an optional dependency of `pdfjs-dist`. Without it the probe prints `skip:` and the run stays green.

### interactive-run.sh

Drives the sidecar through grant → compile → poll against a scratch copy that holds `maleficium-interactive.sty` (copied from `embed-runtime/tex/`) of `fixtures/interactive/`, then checks the MCP `widgets` list (every widget, rects, sources, stale and missing sidecars as errors), the `.mfw` sidecar, the named PDF annotations, the table row cap, the failing documents (bad ids, missing files, no alt, an unknown option, a poster parameter on the wrong widget type, a malformed size), and that nothing landed in the project. Exits non-zero when a check misses. `POLL_ROUNDS` bounds polling; the table text checks need `mutool` and skip with a reason without it.

### export-run.sh

Drives the sidecar through grant → compile of a scratch copy of `fixtures/interactive/` plus the package, then `export_bundle` in every profile (`folder`, `single-file`, `hosted`). Checks each manifest against the committed schema (`src-tauri/core/schemas/paper-bundle-1.schema.json`, validated with python `jsonschema`, required), the sha256 of the pdf and of every asset against both the bundle bytes and the project source, the layout and the inline single-file islands, that each widget is one inline document with its policy first, destinations inside the project refused in all profiles (nothing written), the single-file size cap (a small configured cap, then a real 51 MiB model against the 50 MiB default), a remote-only asset refused without approval, that MCP has no way to approve a download (extra fields refused, no approval field in the tool schema), that the project is unchanged, and that re-export replaces only an earlier bundle. It also calls `preview_bundle` (returns an existing `index.html` outside the project under the app data dir, opens nothing, the next call replaces the previous scratch folder, project porcelain unchanged). Then a two-widget variant (`embed.tex`) declares frame origins: `fig-embed` in its `widget.json`, `fig-macro` with `framedomains=`. The `widgets` list and `widgets_status` carry each widget's own origins, and in single-file and folder each widget document's policy frames only its own origin, the manifest records it as `csp`, and the single-file reader's `frame-src` is exactly the union (the folder reader keeps `frame-src 'self'`). The last step repeats the exports in a process inside an empty network namespace (`unshare -rn`); without that support it prints `skip:` and the cargo test that core has no http client stands in. Exits non-zero when a check misses. `POLL_ROUNDS` bounds polling.

### playground-run.sh

Drives the sidecar over `fixtures/playground/`, the showcase paper for interactive features: one source that compiles to a PDF now and is meant to become HTML later. The paper uses every widget macro the package offers (`\interactivemodel`, `\interactivevideo`, `\interactivetable` from CSV, `\interactivechart` from a Vega-Lite spec, `\interactive` for an HTML folder) with alt text, captioned and uncaptioned floats, plain figures, `subcaption` subfigures, display math, a theorem and proof, citations from `refs.bib`, cross-references, and a section explaining each widget. The run installs the package, compiles with `networked` (so the first run can fetch `subcaption`; later runs are offline), then asserts a clean PDF (no TeX errors, no undefined references, bibliography resolved), the `.mfw` sidecar, one named annotation per widget, the MCP `widgets` list (all seven widgets, all five types, rects, sources, options, labels), and that only the installed package landed in the project. The model, chart and html widgets carry no `poster=`: the app renders their posters (the html one once approved), so the PDF shows the real widget. The video's `figures/clip.png` is a frame of `clip.mp4`, written by `fixtures/gen-media.py`; the `.glb` (a colored cube) and `.mp4` (a test pattern) are small real files written by `fixtures/gen-media.py` (CC0, no third-party asset). The table text checks need `mutool` and skip with a reason without it. `POLL_ROUNDS` bounds polling.

### poster-run.sh

Compiles `fixtures/interactive/posters.tex` over the sidecar and checks the canonical poster parameters in the `widgets` list, then starts its own Xvfb display and drives the app's headless renderer (`maleficium --render-posters <root>`: one JSON request per stdin line, one JSON result per stdout line; no editor window, each render in a hidden window destroyed afterwards). Checks: the fixture model from a `pos/target/up` camera and from a matrix camera gives two different, non-blank 320x240 PNGs; `background=ffffff` paints white and `background=transparent` leaves alpha 0; the chart fills its frame at `scale=2`; the debug-only `hang@test` runtime is stopped at a 3000 ms limit with no PNG written, its WebKit web process is gone, and the next render still works; html and table widgets are refused; the renderer exits non-zero after a failure; the project, explicit posters included, is unchanged; an approved html widget with static and scripted `<link rel=preconnect>` to a loopback listener renders while the listener sees no TCP connection, and every render window logs `LinkPreconnect` among the features it turned off (red before the renderer set the flag: 4 accepts). Needs `Xvfb` and a debug build (the hanging runtime does not exist in release builds). `POSTER_SHOTS=<dir>` keeps the PNGs.

### approval-run.sh

Spawns `maleficium-mcp`, compiles a scratch copy of `fixtures/interactive/`, and checks the approval boundary for html widgets end to end: `widgets_status` and `widget_check` report an unapproved html widget as `approval_required` (a normal result, never an error) with the cause, panel and what the agent must not do; no tool or parameter can approve, revoke or set auto-approval; files in the project that claim approval are ignored; and the approval store in the app data dir (written the way the desktop app's Approve writes it) is what counts: approved, edited, auto mode, a widened origin, and a corrupt store failing closed. The project is never written.

### posters-cache-run.sh

Compiles a scratch project (the interactive fixture plus `auto.tex`, whose model and chart have no `poster=`) over the sidecar with the app's headless renderer beside it, under a private Xvfb. Checks: the first compile renders both posters after the engine run, into `.maleficium/posters/`, and compiles again so its PDF embeds them (`pdfimages`), with a README and a map; an unchanged compile renders nothing; deleting `.maleficium` regenerates it; an edited chart's compile renders the new poster, embeds it and collects the old one; the project shows only the package and `.maleficium/`, search and `find_files` never list the cache, and a hand-edited README survives; `export_zip` carries the cache and the unzipped paper compiles with its posters with no app and no display; the bundle export takes the cached posters. Needs `Xvfb` and `pdfimages` (poppler-utils). The watcher half runs as a core test.

### reader-run.mjs

The reader page (the reflowed article with its widgets mounted), in a headless browser (`node e2e/reader-run.mjs --single <file.html> --folder <dir> [--browser chromium|firefox|webkit]`, Firefox by default; `export-run.sh` exports both profiles and runs it in all three). Install the browsers with `npx playwright-core install chromium firefox webkit`; Playwright's WebKit is the WPE port and on Linux may need `libmanette`, `libbacktrace` and `libhidapi-hidraw` on the host. Serves the single-file bundle and the folder bundle over http, loads each `index.html` (and the single-file one from file://), and checks: the article has its title and section headings, there is one contents list (the article's own nav, every link to a real section, no second client-built one), footnotes are collected into one list, the pdf is offered as a download and not embedded, each poster fills its frame contained at the widget's aspect ratio, every widget frame is mounted once inside the article (in document order; the manifest keeps the pdf's order, which may differ), each is `sandbox="allow-scripts"` exactly, every widget reaches ready and its poster is replaced, each has a caption (its own or its float's), the reader's meta CSP equals the exporter's, a folder bundle from file:// says it is unsupported and mounts nothing, scripts off leaves every poster visible, only bundle files are requested, and the reader's own page reports no CSP violation. Controls, in each profile and again on a copy with the reader CSP removed: an unsandboxed frame and an `allow-scripts allow-same-origin` frame read their parent (red), `allow-scripts` alone and the real widget frames cannot, and a sandboxed widget navigating its own frame to a loopback listener is stopped by the reader's CSP but reaches the listener without it (red). With `--embed-single`, `--embed-folder`, `--embed-ports A,B,U`, `--cert` and `--key` (export-run.sh passes them) it adds the declared-origin cells: three https listeners on 127.0.0.1 with one self-signed certificate (the browser context sets `ignoreHTTPSErrors`; the origins are `https://127.0.0.1:<port>`, so the https-only rule needs no test seam) play `fig-embed`'s declared origin A, `fig-macro`'s B and an undeclared U. Over http, from file:// and in the folder, A and B load and render in their own widget, while U and A framed by `fig-macro` send no request, and neither does `fig-macro` navigating its own frame to A (the single-file reader mounts each widget in a wrapper document whose `frame-src` names only that widget's origins; the folder reader's `frame-src 'self'` holds it there). Red controls on copies: without the widget policies the wrappers still keep `fig-macro` from A (single-file) and both reach everything (folder); without the reader policy each widget's own and its wrapper's still hold; with the wrapper's policy opened to `*` the self-navigation reaches A through the reader's union; without the reader's `frame-src` union a single-file widget cannot frame even its declared origin; with no policy at all (wrapper included) U loads. Exits non-zero when a check misses.

### widget-runtimes/run.mjs

Plays the reader in headless Firefox (`node e2e/widget-runtimes/run.mjs [--shots <dir>]`, no sidecar needed; `ffmpeg` writes a webm test clip, since a stock Firefox has no H.264 decoder). Each of the committed `model` and `video` runtimes gets the exporter's widget policy (read from `fold.rs`) and runs in `<iframe sandbox="allow-scripts" srcdoc>`, fed the fixture bytes over the bridge. Checks: the load from a null origin, a non-blank picture that follows the theme, orbiting by drag, bad input ending in an `error` status, a forged `init` and `theme` from a sibling frame ignored, and no request reaching a loopback listener. Red controls: an unsandboxed frame reads its parent, and a runtime that references an external script, fetch and image reaches the listener once the policy is removed but not under it. Exits non-zero when a check misses.

### stills-run.py

Launches the real app under Xvfb with a throwaway HOME, drives it with keyboard shortcuts, and captures a PNG per state into `STILLS_OUT`. Needs Xvfb, xdotool, and ImageMagick.

```sh
STILLS_OUT=/tmp/stills ./e2e/stills-run.py
```

There are no pixel assertions; the stills are for human review. It is the one harness that starts the frontend, so after each state it also asserts the app's event log: one `log.open` per launch, valid JSONL, and the expected events for that state (saves, compiles, PDF loads, page renders, zoom, the pre-compile warnings panel, and reloads or conflicts when an open file changes on disk).

The cold-compile state reads the TeX bundle through `bundle-mirror.py`, a local cache that only needs the network on its first fill.

### package-smoke.py

The one smoke for every OS, run by CI after each build. Only installing differs: on Linux the `.deb` goes into a clean `ubuntu:24.04` container (so its dependencies, not a build host's, make it run), with payload checks and no `libwayland` in the AppImage; on macOS the `.dmg` is mounted and the `.app`'s signature and architecture checked; on Windows the NSIS setup installs silently. Then everywhere: the installed MCP server, run both as `maleficium-mcp` and as the app with `--mcp` (on Linux the AppImage too), names itself, compiles a page with its bundled engine, and answers a SyncTeX lookup; and the app's first launch must log the welcome-project open (`log.open`, `template.welcome`, `project.open`, `index.open`, `file.open`) with no error events, so a blank window fails. It saves a screenshot of that launch (on Linux, of the `.deb` launch maximized under Xvfb; CI uploads it as `screenshot-*`). It does not check how anything looks.

```sh
python3 e2e/package-smoke.py <package-dir> [--shots DIR]
```

A first launch needs no recent projects, so on macOS and Windows run it as a fresh user (CI runners are).

### papers-run.py

Compiles each real paper in `fixtures/vendored/` over the sidecar and prints a scoreboard: result, error and warning counts, pre-compile findings, and the first blocker. Under each paper it lists every diagnostic the app reported, as the app reported it, with its location (`--brief` hides them); `--json` records them all, so two runs can be diffed. A paper passes when the compile succeeds with no error diagnostics; warnings are counted, not judged. Each paper's `fixture.json` names its main file and whether it is expected to pass today, so the run is a ratchet: it fails when an expected pass breaks, and when an expected failure starts passing, so the expectation gets flipped to lock the gain in. The engine cache persists in `/var/tmp/maleficium-papers-cache-<uid>` (`PAPERS_CACHE`), so only the first run needs the network.

```sh
python3 e2e/papers-run.py [--paper NAME ...] [--json OUT] [--brief] [--bin PATH]
```

### agent-run.py

A real model does a LaTeX task through the MCP server, and the result is judged only on what the run leaves behind. Each scenario in `e2e/agent-scenarios/` names a built-in template, the edits that make it a fixture, a prompt, and oracles. Two runners are supported (`--runner`, default `opencode`); the scenario format and oracles are the same for both. Per run the harness:

1. generates the fixture from `src-tauri/templates` into a fresh run dir;
2. drives the agent headless inside `bwrap`, with the whole host read-only except the run dir (and, for opencode, its own state dirs), and a per-run MCP config that starts the server under test with `--mcp` and a scratch `HOME`;
3. judges the result after the agent exits: it compiles, references and citations resolve, the outline and file contents meet the spec, bytes are restored after an undo, nothing outside the project changed, and the run's own record of MCP calls shows the required tools succeeding in order. The model's prose is never read, except for one machine line a scenario's prompt asks for (`view_shows` below).

```sh
cargo build --manifest-path src-tauri/Cargo.toml --bin maleficium
python3 e2e/agent-run.py --self-test                    # oracles vs. solutions, no model
python3 e2e/agent-run.py -n 3                           # opencode, openrouter/meta/muse-spark-1.3, source build
python3 e2e/agent-run.py -n 3 --model openrouter/meta/muse-spark-1.3 \
    --bin ../out/Maleficium_0.1.1_amd64.AppImage         # opencode, explicit model, packaged build
python3 e2e/agent-run.py --runner claude -n 3            # claude -p, claude-sonnet-5, source build
```

- The opencode runner needs `opencode` (signed in to the model's provider) and `bwrap`. The claude runner needs the `claude` CLI, a token from `claude setup-token` (in `CLAUDE_CODE_OAUTH_TOKEN`, or in `~/.config/maleficium/claude-oauth-token` with mode 0600), and `bwrap`; it defaults to `claude-sonnet-5` and finds `claude` on `PATH` (override with `--claude-bin`). Runs are manual only, never in CI or pre-commit.
- Results go to `--out` (default `/var/tmp/maleficium-agent-runs/<time>`): a dir per run with `result.json` (oracles, metrics, the MCP call record), `events.jsonl` (the runner's own transcript, see below), and the project as the agent left it; plus `summary.md`. Opencode runs also keep a live `opencode.log`.
- A run that hits its scenario's `timeout_s` is killed and reported as a timeout.
- Two sweeps can run at once if the second sets `AGENT_MIRROR_PORT` (default 18790) to a free port for its bundle mirror.
- `--budget` (default $5) stops the whole `-n` sweep once the cost the runner reports adds up to it. For the claude runner, `--max-turns` (default 40) and `--max-budget-usd` (default $2) are an additional hard per-run cap, passed straight to `claude -p`.
- Each scenario's engine cache is warmed once, cold, by compiling its solution (through `bundle-mirror.py` for the source build; online for a packaged build), and kept in `/var/tmp/maleficium-agent-cache`. Each run gets a copy.
- Both runners write their transcript to `events.jsonl`, one event per line, as it streams, so a stalled run is visible before it times out. Claude runner lines carry an added `_ts_ms` (epoch ms at read time) for syncing a screen recording to the transcript.
- Another harness can run one scenario with `run_scenario_once(...)` and get the same `result.json` and `events.jsonl`.
- `--bin source` runs `maleficium --mcp` from `CARGO_TARGET_DIR` (default `src-tauri/target`), or `maleficium-mcp` there when only the server is built.

#### MCP App Views

`show-snippet` asks for a table from the paper through the `snippet` tool, whose result renders in the snippet View. Its oracles replay the agent's last successful `snippet` call on the project it left:

- `view_resource`: the tool's `_meta.ui.resourceUri` names the View, and `resources/read` returns it as a self-contained `text/html;profile=mcp-app` page.
- `snippet_shows_label`: the call lands on the table's page and region.
- `snippet_parity`: as made and with `with_image`, from a client that declares the MCP Apps extension and one that does not, the text content is the `structuredContent`, and both clients get the same text and the same image.
- `view_shows`: `apps-host/render.mjs` renders the call in the View in headless Firefox (the same host page as `apps-host/run.mjs`), saves `view/snippet.png` and `view/render.json` in the run dir, and checks the View shows the region with a `page N of M` header that the answer's `SHOWN: page N of M` line repeats.

A scenario with a `script` also runs under `--self-test` with a scripted fake agent: no model, the same `events.jsonl` shape as the claude runner, judged by every oracle. As scripted it must pass; each `tampered` variant (a wrong answer line, a skipped call) must fail the oracles it names. The View steps need `node` and a Firefox for playwright-core (`npx playwright install firefox`). Offline, with warm caches:

```sh
MIRROR_OFFLINE=1 python3 e2e/agent-run.py --self-test --scenario show-snippet
```

#### Claude runner isolation

The CLI never sees the real `~/.claude`. Each run gets an empty scratch `HOME` and the `setup-token` token in its environment. No credentials file is copied, so run dirs hold no secret and the run cannot rotate your login's refresh token. `--bare` is not used because it accepts only API keys. Also:

- `--setting-sources ""`: no settings files are loaded.
- `--restricted`: file tools stay inside the project; shell and web tools are dropped.
- `--strict-mcp-config` with a config naming only `maleficium`, so no other MCP server, including managed ones, is reachable.
- `--allowedTools`: the server's tools, read from its `tools/list`, plus the file tools. Anything else is denied, not prompted.
- `--tools`: trims the advertised built-in tools, which otherwise dominate prompt cost.
- `bwrap`: the host is read-only except the run dir.

- A failure caused by a confusing tool description or error message is a finding about the MCP server, not the model.

### codrive-run.py

The app and an agent on one project at once. The agent's MCP server and the app are separate processes that share only the filesystem: the project, and the engine outputs under one scratch `HOME` both use. Per run the harness generates `agent-scenarios/codrive/codrive.json`'s fixture, launches the real app on it under Xvfb, compiles it once from the app, then runs the agent through an `agent-run.py` runner with the app open. In `contested` mode (the default) it first types into `conclusion.tex`, a file the agent is asked to rewrite, a key every 0.5 s so the buffer stays unsaved; after the agent exits it compiles from the app with the conflict still open, then reloads from disk.

Oracles from the app's event log: the preview reloaded on the agent's compiles and shows the last one; every file the agent changed was seen as an outside change; clean open buffers took the agent's text; the app wrote nothing into the project while the agent worked; the unsaved buffer raised a conflict and held its autosave rather than being reloaded over or saved over the agent's file. `agent-run.py`'s artifact oracles then judge the project.

```sh
python3 e2e/codrive-run.py --runner script              # the solution, no model: checks the harness
python3 e2e/codrive-run.py -n 3 --model openrouter/meta/muse-spark-1.3
python3 e2e/codrive-run.py --runner claude --mode showcase --fresh
python3 e2e/codrive-run.py --mode race                  # no agent: outside writes timed against autosave
```

- `--mode showcase` is hands off, for recordings; `--fresh` opens the project never compiled, so the agent's first compile is the first pdf the preview shows. `--mode race` writes the open file from outside at 0.9 to 1.4 s after the user's last key, around the 1.2 s autosave, and checks both the outside edit and the key survive.
- It builds the app into `CARGO_TARGET_DIR` (default `/var/tmp/maleficium-codrive-target`) and serves vite on `CODRIVE_PORT` (1423), with Xvfb on `CODRIVE_DISPLAY` (`:97`) at `CODRIVE_SCREEN` (1920x1080) and the bundle mirror on `CODRIVE_MIRROR_PORT` (18791), so it runs beside a stills run or a dev app. It takes the stills display lock.
- Results go to `--out` (default `/var/tmp/maleficium-codrive-runs/<time>`): per run `result.json`, `timeline.jsonl`, `capture.json`, the app's `app-events.jsonl`, stills (`01-before.png`, `02-conflict.png`, `03-after.png`), and the agent's run dir under `agent/`.
- Screen recording: the window is pinned at 0,0 at the full screen size with no window manager, so the display is the app. `capture.json` names the display and geometry; `timeline.jsonl` holds epoch-ms marks (window placed, project open, agent start and exit, conflict, resolution) on the same clock as the app log and the agent's transcript. `CODRIVE_CAPTURE_CMD`, if set, starts through `sh` just before the agent with `DISPLAY`, `CODRIVE_SCREEN` and `CODRIVE_RUN_DIR` set, and gets SIGINT when the run ends:

```sh
CODRIVE_CAPTURE_CMD='exec ffmpeg -loglevel error -f x11grab -video_size $CODRIVE_SCREEN -framerate 30 \
    -i $DISPLAY -pix_fmt yuv420p $CODRIVE_RUN_DIR/screen.mp4' python3 e2e/codrive-run.py --mode showcase
```

### showreel/

A live agent-driving take in the open app: one agent session resumed for each beat of `scenarios/coffee.json` while a director follows the transcript in the app for the camera, then each beat's snapshot is judged on `agent-run.py` oracles. Shared setup (app build, vite, bundle mirror, display handling) lives in `e2e/harness.py` with codrive.

```sh
python3 e2e/showreel/run.py --camera-test <project>   # no agent: the camera's moves on a finished paper
python3 e2e/showreel/run.py --agent print -n 3        # claude -p, off camera
python3 e2e/showreel/run.py --agent opencode -n 3     # headless opencode, one session across beats
python3 e2e/showreel/run.py                           # interactive Claude Code in tmux, on camera (default)
```

- Three agents behind one interface (`agents.py`): `print` (`claude -p`), `tmux` (interactive Claude Code in an xterm on its own display, recorded beside the app), and `opencode` (headless `opencode run`, resumed with `--session`). Every turn streams its transcript to `events.jsonl` and hands the director Claude-shaped tool_use/tool_result envelopes, whatever runner produced them — adding a runner means a new class, no director changes. The tmux runner tails Claude Code's own session log: `print` is the supported path, tmux is best-effort, and a breaking Claude Code change gets resolved if one lands.
- Runs are manual only, never in CI or pre-commit. Takes spend model credits: `--budget` (default $10) stops the sweep, `--beat-budget` caps each `claude -p` beat. `--agent opencode` defaults to `openrouter/meta/muse-spark-1.3` (verified 2026-09-29). There is no working free model id now: `opencode/muse-spark-1.3-contributor-free` returns 403 without an OpenCode Console login, and `openrouter/meta/muse-spark-1.3-contributor` no longer routes.
- It builds the app into `CARGO_TARGET_DIR` (default `/var/tmp/maleficium-showreel-target`) and serves vite on `SHOWREEL_PORT` (1424), with Xvfb on `SHOWREEL_DISPLAY` (`:96`) at `SHOWREEL_SCREEN` (1536x864), the agent's terminal on `:95`, and the bundle mirror on `SHOWREEL_MIRROR_PORT` (18792). The project lives at `SHOWREEL_HOME` (default `/tmp/barista`), wiped only if it holds the `.showreel-home` marker. It takes the stills display lock.
- Beats are judged per snapshot with `agent-run.py`'s `Judge`; two sequential forms (`more_addplots_than_before`, `table_columns_grew`) translate to absolute thresholds against the previous beat. A take passes only if every beat does, so a video can state an honest pass rate over its takes.
- Per take (`take-N/`): `screen.mp4` (and `claude.mp4` for tmux), `captions.srt`, `events.jsonl`, `camera.jsonl`, `timeline.jsonl`, the app's `app-events.jsonl`, per-beat snapshots, and `result.json`. `--preview` stitches a tmux take's two recordings side by side, sped up; `--suggest-shots` prints candidate edit shots from the shared-clock logs. Per-take edit scripts stay out of the repo: only these generic transforms belong here.
- The camera moves through the app's dev channel (`src/lib/devCamera.ts`, tested): the harness appends numbered moves to `SHOWREEL_CAMERA_FILE`, the dev server serves them at `/__camera`, and the app runs open/line/page and four allowlisted view commands through its own handlers. No keystrokes, so no move can change a document; release builds carry no camera path. `SHOWREEL_DEBUG=1` screenshots every move.

## Reading the app's event log

The app records every event as JSONL, one object per line, truncated at each launch:

```sh
L=~/.local/share/io.github.wahahayes.maleficium/maleficium-log/events.jsonl
cat "$L"                                  # the whole run
grep '"action":"compile.finish"' "$L"     # one kind of event
```

Match on `event.action` rather than the message text. For a stills run, set `STILLS_HOME=/tmp/x` and read the log under `$STILLS_HOME` instead.

## Conventions

- Harnesses find the repo root from their own path. No hardcoded absolute paths, and no writes outside the OS temp dir.
- Never commit large fixtures: reuse `e2e/fixtures/simple/` or generate into the temp dir at runtime. Harnesses copy it to a scratch dir and never write to it.
- Real papers live in `e2e/fixtures/vendored/`, each with its source, license, and current build status; see its README before adding one.
- Name tests for what they check, not for when they were written.
