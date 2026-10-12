# e2e — end-to-end harnesses

These scripts check the built app and its automation sidecar from the outside. They are test code and never ship in the app bundle. Run every command from the repo root.

## Finding and running a harness

Each harness is a script or folder in this directory. Shared helpers sit beside them.

Each script's header comment or docstring says what it proves, what it needs, and how to run it. Read it before running the script. Shell harnesses take no arguments. They print `ok:` or `FAIL:` per check and exit non-zero when a check misses.

Most harnesses drive the `maleficium-mcp` sidecar over stdio and build it first. Those that compile through the engine need `sh scripts/build-engine.sh` to have run.

## Display

Harnesses that open the app or a webview start their own Xvfb display, so they run beside a dev app. Their headers name the display and port variables to move them. `package-smoke.py` on Linux runs under Xvfb in Docker.

The browser suites run headless, but headless Firefox has no WebGL without a display. Run them under `xvfb-run -a` on a machine with no display, as `scripts/ci-e2e.sh` does.

## Harnesses that spend model credits

`agent-run.py`, `codrive-run.py` and `showreel/run.py` drive a real model. They run by hand only, never in CI or pre-commit. Each takes `--budget` and stops starting runs once the reported cost reaches it. `agent-run.py --self-test` and `codrive-run.py --runner script` check the harness with no model.

Two runners are supported: `opencode` (signed in to the model's provider) and `claude`. Both need `bwrap`. The claude runner needs a token from `claude setup-token`, in `CLAUDE_CODE_OAUTH_TOKEN` or in `~/.config/maleficium/claude-oauth-token` with mode 0600. Results go to `--out`, by default a timestamped dir under `/var/tmp`. Each run streams the runner's transcript to `events.jsonl`, so a stalled run is visible before it times out.

### MCP App Views

The `show-snippet` scenario checks the `snippet` tool and its View. Its `view_shows` oracle renders the agent's last call with `apps-host/render.mjs` in headless Firefox, so it needs `node` and a Firefox for playwright-core (`npx playwright install firefox`). A scenario with a `script` also runs under `--self-test` with a scripted fake agent. Offline, with warm caches:

```sh
MIRROR_OFFLINE=1 python3 e2e/agent-run.py --self-test --scenario show-snippet
```

### Claude runner isolation

The CLI never sees the real `~/.claude`. Each run gets an empty scratch `HOME` and the `setup-token` token in its environment. No credentials file is copied, so run dirs hold no secret and the run cannot rotate your login's refresh token. `--bare` is not used because it accepts only API keys. Also:

- `--setting-sources ""`: no settings files are loaded.
- `--restricted`: file tools stay inside the project; shell and web tools are dropped.
- `--strict-mcp-config` with a config naming only `maleficium`, so no other MCP server, including managed ones, is reachable.
- `--allowedTools`: the server's tools, read from its `tools/list`, plus the file tools. Anything else is denied, not prompted.
- `--tools`: trims the advertised built-in tools, which otherwise dominate prompt cost.
- `bwrap`: the host is read-only except the run dir.

A failure caused by a confusing tool description or error message is a finding about the MCP server, not the model.

### Showreel

`showreel/run.py` records an agent writing a document in the open app while a director moves the camera through the app's dev channel (`src/lib/devCamera.ts`). It never sends keystrokes, and release builds carry no camera path. `--agent` picks `tmux` (interactive Claude Code on camera, the default), `print` (`claude -p`) or `opencode`. The tmux agent tails Claude Code's own session log, so it is best-effort; `print` is the supported path. `--beat-budget` caps each `claude -p` beat. Per-take edit scripts stay out of the repo.

## In CI

The `e2e` job in `.github/workflows/build.yml` runs the interactive-papers suites on Linux through `scripts/ci-e2e.sh`. That script's header lists the suites, what they need, and how to run one locally. It works the same way locally once `scripts/build-engine.sh` has run. Other jobs in `build.yml` run further harnesses; read it for which.

## Running in isolation

Run harnesses through `worktree-run.sh`, which checks out a pinned commit into a separate worktree so edits in your checkout cannot disturb a run:

```sh
./e2e/worktree-run.sh [<ref>] -- ./e2e/stills-run.py
```

It carries your uncommitted `e2e/` changes into the worktree and builds into its own Cargo target dir, so it never touches your dev build. Its header covers the rest.

## Reading the app's event log

The app records every event as JSONL, one object per line, truncated at each launch:

```sh
L="${XDG_DATA_HOME:-$HOME/.local/share}/io.github.wahahayes.maleficium/maleficium-log/events.jsonl"
cat "$L"                                  # the whole run
grep '"action":"compile.finish"' "$L"     # one kind of event
```

Match on `event.action` rather than the message text. Harnesses that launch the app give it a scratch `HOME`, so read the log under that `HOME` instead (for a stills run, set `STILLS_HOME`).

## Conventions

- Harnesses find the repo root from their own path. No hardcoded absolute paths.
- Output goes to the OS temp dir (caches and run results may go to `/var/tmp`) and is never committed.
- Never commit large fixtures: reuse `e2e/fixtures/simple/` or generate into the temp dir at runtime. Harnesses copy fixtures to a scratch dir and never write to them.
- Real papers live in `e2e/fixtures/vendored/`; see its README before adding one.
- Name tests for what they check, not for when they were written.
