#!/usr/bin/env python3
"""agent-run.py — a real LLM agent does LaTeX tasks through the MCP server.

Each scenario (e2e/agent-scenarios/*.json) names a built-in template, the
edits that turn it into a fixture, a prompt, and oracles. Per run:
  1. the fixture is generated from src-tauri/templates into a fresh run dir;
  2. an agent runner (opencode or claude, see --runner) drives the server
     under test (`maleficium --mcp`) headless in a bubblewrap sandbox (the
     host is read-only; only the run dir and the runner's own state are
     writable), with a per-run MCP config and a scratch HOME for the server,
     prewarmed with the TeX bundle;
  3. after the agent exits (or its timeout kills it), the oracles judge the
     artifacts: compiles, clean log, outline, file contents, bytes restored,
     nothing written outside the project, and the run's own tool-call record.
Pass/fail never reads the model's prose. Results land in --out: one dir per
run (result.json, events.jsonl, project/, plus opencode.log for the opencode
runner), plus summary.json and summary.md.

Usage:
  python3 e2e/agent-run.py [--runner opencode|claude] [--bin source|<AppImage>]
                           [--model M] [-n N] [--scenario ID ...] [--out DIR]
                           [--budget USD]
  python3 e2e/agent-run.py --self-test [--bin ...]   # oracles vs. solutions, no model
Needs: python3, bwrap, and the chosen runner's CLI, authenticated
(opencode: `opencode auth`; claude: `claude auth login`, or `claude setup-token`).
The source build needs
`cargo build --manifest-path src-tauri/Cargo.toml --bin maleficium` first.
Manual only: it spends model credits. See e2e/README.md.
"""
import argparse, glob, hashlib, json, os, re, shutil, signal, subprocess, sys, tempfile, threading, time

from mcp_client import McpClient

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
TEMPLATES = os.path.join(ROOT, "src-tauri", "templates")
SCENARIOS = os.path.join(HERE, "agent-scenarios")
IDENT = "io.github.wahahayes.maleficium"
CACHE_ROOT = "/var/tmp/maleficium-agent-cache"
MIRROR_CACHE = "/var/tmp/maleficium-bundle-mirror"
MIRROR_PORT = int(os.environ.get("AGENT_MIRROR_PORT", "18790"))  # override to run beside another sweep
FREE_MODEL = "opencode/muse-spark-1.3-contributor-free"
CLAUDE_MODEL = "claude-sonnet-5"
CLAUDE_BUILTIN_TOOLS = ["Read", "Edit", "Write", "Glob", "Grep"]  # no Bash, no WebFetch


def say(msg):
    print("agent-run: " + msg, flush=True)


def die(msg):
    sys.exit("agent-run: " + msg)


# ---- the server under test ---------------------------------------------------


class Server:
    """How to start `maleficium --mcp` for one binary kind, and its env."""

    def __init__(self, spec, mirror_url):
        if spec == "source":
            self.kind = "source"
            self.cmd = [os.path.join(ROOT, "src-tauri", "target", "debug", "maleficium"), "--mcp"]
            if not os.access(self.cmd[0], os.X_OK):
                die("no source build; run cargo build --manifest-path src-tauri/Cargo.toml --bin maleficium")
            # debug builds read the bundle through the local mirror
            self.env = {"MALEFICIUM_DEV_BUNDLE_URL": mirror_url}
        else:
            self.kind = "packaged"
            self.cmd = [os.path.abspath(spec), "--mcp"]
            if not os.access(self.cmd[0], os.X_OK):
                die("not executable: %s" % spec)
            # the sandbox has no FUSE; release builds ignore the dev bundle URL
            self.env = {"APPIMAGE_EXTRACT_AND_RUN": "1"}

    def environ(self, home):
        env = {k: v for k, v in os.environ.items() if not k.startswith("XDG_")}
        env.update(self.env, HOME=home)
        return env


class Mcp(McpClient):
    """A scripted MCP client for oracles and cache warming."""

    def __init__(self, server, home):
        super().__init__(server.cmd, "agent-run", env=server.environ(home), stderr=subprocess.DEVNULL)

    def call(self, name, args):
        """(ok, structuredContent or error text)"""
        return self.tool(name, args)

    def list_tools(self):
        """Every tool name the server actually advertises, read from the live
        server rather than hardcoded, so the allowlist never drifts from the
        real MCP surface."""
        return [t["name"] for t in self.request("tools/list", {})["result"]["tools"]]

    def compile(self, root_id, rel, timeout=600):
        ok, job = self.call("compile_run", {"root_id": root_id, "rel": rel})
        if not ok:
            return {"status": "refused", "error": job}
        deadline = time.time() + timeout
        rec = {"status": "running"}
        while rec.get("status") == "running" and time.time() < deadline:
            ok, rec = self.call("compile_poll", {"job_id": job["job_id"], "tail_lines": 40, "wait_ms": 30000})
            if not ok:
                return {"status": "poll-error", "error": rec}
        return rec

    def close(self):
        try:
            super().close(timeout=20)
        except Exception:
            self.p.kill()


# ---- fixtures ------------------------------------------------------------------


def load_scenarios(ids):
    found = {}
    for path in sorted(glob.glob(os.path.join(SCENARIOS, "*.json"))):
        with open(path) as f:
            s = json.load(f)
        found[s["id"]] = s
    if ids:
        missing = [i for i in ids if i not in found]
        if missing:
            die("no such scenario: %s (have %s)" % (", ".join(missing), ", ".join(found)))
        return [found[i] for i in ids]
    return list(found.values())


def apply_edits(project, edits):
    for e in edits:
        path = os.path.join(project, e["file"])
        with open(path) as f:
            text = f.read()
        if "append" in e:
            text += e["append"]
        else:
            if text.count(e["find"]) != 1:
                die("edit to %s: expected one %r, found %d" % (e["file"], e["find"], text.count(e["find"])))
            text = text.replace(e["find"], e["replace"])
        with open(path, "w") as f:
            f.write(text)


def make_fixture(scenario, project, solved=False):
    """The template plus the scenario's edits and files (plus its solution)."""
    shutil.copytree(os.path.join(TEMPLATES, scenario["template"]), project)
    os.remove(os.path.join(project, "template.json"))
    for rel, text in scenario.get("files", {}).items():
        with open(os.path.join(project, rel), "w") as f:
            f.write(text)
    apply_edits(project, scenario.get("edits", []))
    if solved:
        apply_edits(project, scenario.get("solution", []))


def snapshot(top, skip=()):
    """rel path -> sha256 for every file under top, minus skipped top-level names."""
    out = {}
    for d, dirs, files in os.walk(top):
        rel_d = os.path.relpath(d, top)
        if rel_d == ".":
            dirs[:] = [x for x in dirs if x not in skip]
        for name in files:
            rel = os.path.normpath(os.path.join(rel_d, name))
            if rel in skip:
                continue
            with open(os.path.join(d, name), "rb") as f:
                out[rel] = hashlib.sha256(f.read()).hexdigest()
    return out


# ---- engine cache --------------------------------------------------------------


def start_mirror(outdir):
    log = open(os.path.join(outdir, "mirror.log"), "w")
    p = subprocess.Popen([sys.executable, os.path.join(HERE, "bundle-mirror.py"), str(MIRROR_PORT), MIRROR_CACHE],
                         stdout=subprocess.PIPE, stderr=log, text=True)
    if "listening" not in (p.stdout.readline() or ""):
        die("bundle mirror did not start (port %d held? see %s/mirror.log)" % (MIRROR_PORT, outdir))
    return p


# Every size in every common face, so an agent's new text rarely needs a font
# the warm cache lacks: a warm cache does not fetch a missing OpenType face.
FONT_SOAK = "\n".join(
    "{\\%s x \\itshape x \\bfseries x \\upshape\\mdseries\\slshape x \\upshape\\scshape x \\normalfont\\ttfamily x}\\par"
    % size for size in ["tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE",
                        "huge", "Huge"]) + "\n"


def warm_cache(server, scenario):
    """A HOME whose engine cache holds what the scenario's solved fixture needs.

    Filled by one cold compile (all online, through the mirror for the source
    build) of the solution plus FONT_SOAK, and kept across runs. Per scenario,
    because a second, different document in a warm cache can miss a font that
    is never fetched from the online bundle."""
    home = os.path.join(CACHE_ROOT, server.kind, scenario["id"])
    if os.path.isdir(os.path.join(home, ".cache", IDENT, "maleficium-tectonic")):
        return home
    os.makedirs(home, exist_ok=True)
    with tempfile.TemporaryDirectory(dir="/var/tmp") as t:
        project = os.path.join(t, "p")
        make_fixture(scenario, project, solved=True)
        main = os.path.join(project, "main.tex")
        with open(main) as f:
            text = f.read()
        with open(main, "w") as f:
            f.write(text.replace("\\end{document}", FONT_SOAK + "\\end{document}"))
        mcp = Mcp(server, home)
        try:
            ok, err = mcp.call("grant", {"root_id": "warm", "root": project})
            if not ok:
                die("warm grant: %s" % err)
            rec = mcp.compile("warm", "main.tex")
        finally:
            mcp.close()
    if rec.get("status") != "success":
        shutil.rmtree(home)
        die("warm compile of %s's solution: %s" % (scenario["id"], str(rec.get("log") or rec)[:600]))
    say("engine cache warm for %s (%s)" % (scenario["id"], home))
    return home


def discover_tool_names(server):
    """The server's real tool names, queried live (not hardcoded), for the
    claude runner's --allowedTools list."""
    with tempfile.TemporaryDirectory(dir="/var/tmp") as home:
        mcp = Mcp(server, home)
        try:
            return mcp.list_tools()
        finally:
            mcp.close()


def seed_home(warm, home):
    src = os.path.join(warm, ".cache", IDENT, "maleficium-tectonic")
    dst = os.path.join(home, ".cache", IDENT, "maleficium-tectonic")
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copytree(src, dst, symlinks=True)


# ---- one agent run ---------------------------------------------------------------


def sandbox(run_dir, project, argv, extra_rw=()):
    """bwrap argv: the whole host read-only except run_dir (rw) and any
    extra_rw paths the runner's own client needs for its state. The claude
    runner needs none (its whole HOME is inside run_dir); opencode needs its
    real state dirs bound in (see run_agent_opencode)."""
    cmd = ["bwrap", "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp",
           "--bind", run_dir, run_dir]
    for path in extra_rw:
        os.makedirs(path, exist_ok=True)
        cmd += ["--bind", path, path]
    return cmd + ["--die-with-parent", "--chdir", project] + argv


def run_agent_opencode(scenario, server, model, run_dir, warm):
    project = os.path.join(run_dir, "project")
    home = os.path.join(run_dir, "home")
    make_fixture(scenario, project)
    seed_home(warm, home)
    for rel, text in scenario.get("outside", {}).items():
        with open(os.path.join(run_dir, rel), "w") as f:
            f.write(text)
    config = os.path.join(run_dir, "opencode.json")
    with open(config, "w") as f:
        json.dump({"mcp": {"servers": {"maleficium": {
            "type": "local", "command": server.cmd, "environment": dict(server.env, HOME=home)}}}}, f, indent=2)
    before = snapshot(run_dir, skip=("home",))
    prompt = scenario["prompt"].replace("{project}", project)
    argv = ["env", "OPENCODE_CONFIG=" + config, "opencode", "run", "--standalone", "--auto", "--print-logs",
            "--format", "json", "-m", model, prompt]
    t0 = time.time()
    h = os.path.expanduser("~")
    extra_rw = [os.path.join(h, d) for d in (".local/share/opencode", ".cache/opencode", ".local/state/opencode")]
    with open(os.path.join(run_dir, "events.jsonl"), "w") as out, \
            open(os.path.join(run_dir, "opencode.log"), "w") as log:
        p = subprocess.Popen(sandbox(run_dir, project, argv, extra_rw=extra_rw), stdout=out, stderr=log,
                             stdin=subprocess.DEVNULL, start_new_session=True)
        try:
            code = p.wait(timeout=scenario.get("timeout_s", 600))
            timed_out = False
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.wait()
            code, timed_out = None, True
    wall = time.time() - t0
    return {"exit": code, "timed_out": timed_out, "wall_s": round(wall, 1), "before": before,
            "project": project, "home": home, "prompt": prompt}


CLAUDE_TOKEN_FILE = os.path.expanduser("~/.config/maleficium/claude-oauth-token")


def claude_token():
    """The claude runner's auth: CLAUDE_CODE_OAUTH_TOKEN from the environment,
    else the token file. Both come from `claude setup-token`. Copying
    ~/.claude/.credentials.json instead would leave a secret in every run dir
    and let the child CLI rotate the user's refresh token."""
    token = os.environ.get("CLAUDE_CODE_OAUTH_TOKEN", "").strip()
    if not token and os.path.isfile(CLAUDE_TOKEN_FILE):
        with open(CLAUDE_TOKEN_FILE) as f:
            token = f.read().strip()
    if not token:
        die("no token for the claude runner: run `claude setup-token` and put the token in "
            "CLAUDE_CODE_OAUTH_TOKEN or %s (mode 0600)" % CLAUDE_TOKEN_FILE)
    return token


def run_agent_claude(scenario, server, model, run_dir, warm, tool_names, max_turns, budget_usd, claude_bin):
    """Drive `claude -p` against the server under test, isolated from the
    user's Claude Code config (see e2e/README.md).

    The CLI gets an empty scratch HOME and a long-lived token in its
    environment (see claude_token), never a copy of ~/.claude's credentials,
    loads no settings, and reaches no MCP server but maleficium. `--restricted`
    keeps the file tools inside the project and drops shell and web tools.
    `--bare` is not an option: it accepts only API-key auth. `--tools` trims
    the advertised built-in tools, which otherwise dominate prompt cost.
    """
    project = os.path.join(run_dir, "project")
    mcp_home = os.path.join(run_dir, "mcp-home")
    claude_home = os.path.join(run_dir, "claude-home")
    make_fixture(scenario, project)
    seed_home(warm, mcp_home)
    for rel, text in scenario.get("outside", {}).items():
        with open(os.path.join(run_dir, rel), "w") as f:
            f.write(text)
    os.makedirs(os.path.join(claude_home, ".claude"), exist_ok=True, mode=0o700)
    env = dict(os.environ, HOME=claude_home, CLAUDE_CODE_OAUTH_TOKEN=claude_token())
    config = os.path.join(run_dir, "claude-mcp.json")
    with open(config, "w") as f:
        json.dump({"mcpServers": {"maleficium": {"type": "stdio", "command": server.cmd[0],
                                                 "args": server.cmd[1:], "env": dict(server.env, HOME=mcp_home)}}},
                  f, indent=2)
    before = snapshot(run_dir, skip=("mcp-home", "claude-home"))
    prompt = scenario["prompt"].replace("{project}", project)
    allowed = CLAUDE_BUILTIN_TOOLS + ["mcp__maleficium__" + t for t in tool_names]
    argv = [claude_bin, "-p",
            "--output-format", "stream-json", "--verbose",
            "--max-turns", str(max_turns), "--max-budget-usd", str(budget_usd),
            "--setting-sources", "", "--disable-slash-commands", "--no-session-persistence", "--restricted",
            "--tools", ",".join(CLAUDE_BUILTIN_TOOLS),
            "--mcp-config", config, "--strict-mcp-config",
            "--allowedTools"] + allowed + ["--model", model, prompt]
    events_path = os.path.join(run_dir, "events.jsonl")
    t0 = time.time()
    p = subprocess.Popen(sandbox(run_dir, project, argv), stdout=subprocess.PIPE,
                         stderr=open(os.path.join(run_dir, "claude.stderr.log"), "w"),
                         stdin=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True, env=env)

    def pump():
        """Append each stream-json line to events.jsonl as it arrives, with an
        added `_ts_ms` field (epoch ms) so a screen recording can be synced
        to the transcript. Unparseable lines are kept as `_raw`."""
        with open(events_path, "w") as out:
            for line in iter(p.stdout.readline, ""):
                ts_ms = round(time.time() * 1000, 1)
                line = line.rstrip("\n")
                if not line:
                    continue
                try:
                    e = json.loads(line)
                    if isinstance(e, dict):
                        e["_ts_ms"] = ts_ms
                    else:
                        e = {"_ts_ms": ts_ms, "_raw": line}
                except ValueError:
                    e = {"_ts_ms": ts_ms, "_raw": line}
                out.write(json.dumps(e) + "\n")
                out.flush()

    reader = threading.Thread(target=pump, daemon=True)
    reader.start()
    try:
        code = p.wait(timeout=scenario.get("timeout_s", 600))
        timed_out = False
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, signal.SIGKILL)
        p.wait()
        code, timed_out = None, True
    reader.join(timeout=10)
    wall = time.time() - t0
    return {"exit": code, "timed_out": timed_out, "wall_s": round(wall, 1), "before": before,
            "project": project, "home": mcp_home, "prompt": prompt}


def read_events_opencode(path):
    """Metrics and the tool-call record from opencode's --format json stream."""
    calls, other, texts = [], [], []
    m = {"steps": 0, "tokens_in": 0, "tokens_out": 0, "tokens_reasoning": 0, "cost": 0.0}
    with open(path) as f:
        for line in f:
            try:
                e = json.loads(line)
            except ValueError:
                continue
            part = e.get("part", {})
            if e.get("type") == "step_finish":
                m["steps"] += 1
                t = part.get("tokens") or {}
                m["tokens_in"] += t.get("input", 0) + (t.get("cache") or {}).get("read", 0)
                m["tokens_out"] += t.get("output", 0)
                m["tokens_reasoning"] += t.get("reasoning", 0)
                m["cost"] += part.get("cost") or 0
            elif e.get("type") == "text":
                texts.append(part.get("text", ""))
            elif e.get("type") == "tool_use":
                st = part.get("state", {})
                inner = ((st.get("metadata") or {}).get("metadata") or {}).get("toolCalls")
                if part.get("tool") == "execute" and inner is not None:
                    for c in inner:
                        name = c.get("tool", "")
                        if name.startswith("maleficium."):
                            ok = c.get("status") == "completed"
                            # a failed call's text reaches only the script's output
                            calls.append({"tool": name.split(".", 1)[1], "input": c.get("input") or {},
                                          "ok": ok, "status": c.get("status"),
                                          "error": None if ok else str(st.get("output") or st.get("error"))[:600]})
                else:
                    other.append({"tool": part.get("tool"), "status": st.get("status"),
                                  "input": st.get("input")})
    m["cost"] = round(m["cost"], 6)
    m["mcp_calls"] = len(calls)
    m["mcp_errors"] = sum(1 for c in calls if not c["ok"])
    m["other_calls"] = len(other)
    return m, calls, other, texts


def read_events_claude(path):
    """Metrics and the tool-call record from claude's timestamped stream-json
    (see run_agent_claude's pump()). Same return shape as
    read_events_opencode, so the oracles and summary are runner-agnostic.

    stream-json for `claude -p` is a flat sequence of top-level events, not
    opencode's nested step/part shape: `assistant` messages carry `text` and
    `tool_use` content blocks, the next `user` message carries the matching
    `tool_result` (linked by `tool_use_id`), and one final `result` event
    carries num_turns, total_cost_usd (already summed across every model the
    turn used, e.g. a fast classifier alongside the main model), and
    per-model token counts in `modelUsage`.
    """
    calls, other, texts = [], [], []
    m = {"steps": 0, "tokens_in": 0, "tokens_out": 0, "tokens_reasoning": 0, "cost": 0.0}
    pending = {}  # tool_use id -> {"name": ..., "input": ...}
    with open(path) as f:
        for line in f:
            try:
                e = json.loads(line)
            except ValueError:
                continue
            t = e.get("type")
            if t == "assistant":
                for c in e.get("message", {}).get("content", []):
                    if c.get("type") == "text":
                        texts.append(c.get("text", ""))
                    elif c.get("type") == "tool_use":
                        pending[c.get("id")] = {"name": c.get("name", ""), "input": c.get("input") or {}}
            elif t == "user":
                for c in e.get("message", {}).get("content", []):
                    if c.get("type") != "tool_result":
                        continue
                    info = pending.pop(c.get("tool_use_id"), None)
                    if info is None:
                        continue
                    ok = not c.get("is_error")
                    content = c.get("content")
                    if isinstance(content, list):
                        content = "".join(x.get("text", "") for x in content if isinstance(x, dict))
                    name = info["name"]
                    if name.startswith("mcp__maleficium__"):
                        calls.append({"tool": name[len("mcp__maleficium__"):], "input": info["input"], "ok": ok,
                                      "status": "completed" if ok else "error",
                                      "error": None if ok else str(content)[:600]})
                    else:
                        other.append({"tool": name, "status": "completed" if ok else "error", "input": info["input"]})
            elif t == "result":
                m["steps"] = e.get("num_turns", 0) or 0
                m["cost"] = e.get("total_cost_usd", 0.0) or 0.0
                for mu in (e.get("modelUsage") or {}).values():
                    m["tokens_in"] += mu.get("inputTokens", 0) + mu.get("cacheReadInputTokens", 0) + \
                        mu.get("cacheCreationInputTokens", 0)
                    m["tokens_out"] += mu.get("outputTokens", 0)
                    m["tokens_reasoning"] += mu.get("thinkingTokens", 0)
                if e.get("result"):
                    texts.append(str(e.get("result")))
    m["cost"] = round(m["cost"], 6)
    m["mcp_calls"] = len(calls)
    m["mcp_errors"] = sum(1 for c in calls if not c["ok"])
    m["other_calls"] = len(other)
    return m, calls, other, texts


# ---- oracles ---------------------------------------------------------------------


def read_file(project, rel):
    try:
        with open(os.path.join(project, rel)) as f:
            return f.read()
    except OSError:
        return None


def section_text(text, title):
    m = re.search(r"\\section\*?\{" + re.escape(title) + r"\}", text or "")
    if not m:
        return None
    rest = text[m.end():]
    nxt = re.search(r"\\(section|printbibliography|bibliography|end\{document\})", rest)
    return rest[: nxt.start()] if nxt else rest


def escapes(path):
    return isinstance(path, str) and (path.startswith("/") or path.startswith("\\")
                                      or ".." in re.split(r"[\\/]", path) or re.match(r"^[A-Za-z]:", path))


class Judge:
    """Evaluates one scenario's oracles against a finished run."""

    def __init__(self, scenario, server, project, home, fixture, calls, run_dir=None, before=None, own=()):
        self.s, self.server, self.project, self.home = scenario, server, project, home
        self.fixture, self.calls, self.run_dir, self.before = fixture, calls, run_dir, before
        self.own = own  # extra top-level run_dir names the runner itself owns (not agent output)
        self._mcp = None
        self._compiled = {}

    def mcp(self):
        if self._mcp is None:
            self._mcp = Mcp(self.server, self.home)
            ok, err = self._mcp.call("grant", {"root_id": "judge", "root": self.project})
            if not ok:
                raise RuntimeError("grant: " + err)
        return self._mcp

    def compiled(self, main):
        if main not in self._compiled:
            self._compiled[main] = self.mcp().compile("judge", main)
        return self._compiled[main]

    def close(self):
        if self._mcp:
            self._mcp.close()

    # each returns (ok, detail)
    def o_compiles(self, main):
        rec = self.compiled(main)
        if rec.get("status") != "success":
            return False, "status %s: %s" % (rec.get("status"), str(rec.get("log_tail") or rec)[-300:])
        pdf = rec.get("pdf_path") or rec.get("pdf_url") or ""
        pdf = re.sub(r"^file://", "", pdf)
        if pdf and not os.path.isfile(pdf):
            return False, "no PDF at %s" % pdf
        return True, "success"

    def o_clean_log(self, main):
        """No error or undefined-key diagnostics, and every reference and
        citation resolves in the index too (two sources, so one tool's miss
        cannot pass a broken document)."""
        rec = self.compiled(main)
        if rec.get("status") != "success":
            return False, "did not compile"
        m = self.mcp()
        ok, d = m.call("diagnostics", {"root_id": "judge", "main_rel": main, "max": 200})
        if not ok:
            return False, "diagnostics refused: " + d
        bad = ["%s:%s %s" % (x.get("path"), x["line"], x["message"][:80]) for x in d["diagnostics"]
               if x["severity"] == "error" or re.search(r"undefined|multiply defined", x["message"], re.I)]
        ok, lr = m.call("labels_refs", {"root_id": "judge", "main_rel": main})
        if not ok:
            return False, "labels_refs refused: " + lr
        bad += ["unresolved \\ref{%s} at %s:%s" % (r["key"], r["rel"], r["line"]) for r in lr["refs"] if not r["resolved"]]
        bad += ["duplicate \\label{%s}" % l["key"] for l in lr["labels"] if l["duplicate"]]
        ok, ci = m.call("citations", {"root_id": "judge", "main_rel": main})
        if not ok:
            return False, "citations refused: " + ci
        bad += ["unresolved \\cite{%s} at %s:%s" % (c["key"], c["rel"], c["line"]) for c in ci["cites"] if not c["resolved"]]
        if bad:
            return False, "; ".join(bad[:5])
        return True, "no errors; %d refs and %d cites resolve" % (len(lr["refs"]), len(ci["cites"]))

    def o_outline_has(self, spec):
        ok, doc = self.mcp().call("outline", {"root_id": "judge", "rel": spec["file"]})
        if not ok:
            return False, "outline refused: " + doc
        titles = [r.get("title") or r.get("text") or r.get("name") for r in doc.get("rows", doc.get("entries", []))]
        titles = [t for t in titles if t]
        want = spec["titles"]
        pos = []
        for w in want:
            if w not in titles:
                return False, "missing section %r (have %s)" % (w, titles)
            pos.append(titles.index(w))
        if spec.get("ordered") and pos != sorted(pos):
            return False, "sections out of order: %s" % titles
        return True, "has %s" % want

    def o_file_matches(self, spec):
        text = read_file(self.project, spec["file"])
        if text is None:
            return False, "%s missing" % spec["file"]
        for pat in spec["patterns"]:
            if not re.search(pat, text):
                return False, "%s lacks /%s/" % (spec["file"], pat)
        return True, "all %d patterns" % len(spec["patterns"])

    def o_file_lacks(self, spec):
        text = read_file(self.project, spec["file"]) or ""
        for pat in spec["patterns"]:
            if re.search(pat, text):
                return False, "%s still has /%s/" % (spec["file"], pat)
        return True, "none of %d patterns" % len(spec["patterns"])

    def o_count_equal(self, spec):
        text = read_file(self.project, spec["file"]) or ""
        a, b = (len(re.findall(p, text)) for p in spec["patterns"])
        return a == b and a > 0, "%d vs %d" % (a, b)

    def o_section_matches(self, spec):
        body = section_text(read_file(self.project, spec["file"]), spec["title"])
        if body is None:
            return False, "no section %r" % spec["title"]
        for pat in spec["patterns"]:
            if not re.search(pat, body, re.S):
                return False, "section %r lacks /%s/" % (spec["title"], pat)
        labels = re.findall(r"\\label\{([^}]+)\}", body)
        if spec.get("refs_own_label"):
            if not any(re.search(r"\\(?:[a-zA-Z]*ref)\{" + re.escape(l) + r"\}", body) for l in labels):
                return False, "no label of the section (%s) is referenced in it" % labels
        return True, "all %d patterns" % len(spec["patterns"])

    def o_unchanged(self, files):
        now = snapshot(self.project)
        changed = [f for f in files if now.get(f) != self.fixture.get(f)]
        return not changed, "changed: %s" % changed if changed else "identical"

    def o_restored(self, _):
        now = snapshot(self.project)
        diff = sorted(set(now) ^ set(self.fixture)) + sorted(f for f in now if f in self.fixture and now[f] != self.fixture[f])
        return not diff, "differs: %s" % diff[:6] if diff else "byte-identical to the fixture"

    def o_absent(self, files):
        there = [f for f in files if os.path.exists(os.path.join(self.project, f))]
        return not there, "still present: %s" % there if there else "gone"

    def o_footprint(self, _):
        if self.before is None:
            return True, "n/a"
        skip = ("home", "events.jsonl", "opencode.log", "result.json") + tuple(self.own)
        after = snapshot(self.run_dir, skip=skip)
        outside = {k: v for k, v in after.items() if not k.startswith("project" + os.sep)}
        was = {k: v for k, v in self.before.items() if not k.startswith("project" + os.sep)}
        diff = sorted(set(outside) ^ set(was)) + sorted(k for k in outside if k in was and outside[k] != was[k])
        return not diff, "outside the project: %s" % diff[:6] if diff else "nothing outside the project changed"

    def o_called(self, spec):
        """Each tool, in order, with a successful call after the previous one's first success."""
        i = 0
        for want in spec:
            while i < len(self.calls) and not (self.calls[i]["tool"] == want and self.calls[i]["ok"]):
                i += 1
            if i == len(self.calls):
                seen = ["%s%s" % (c["tool"], "" if c["ok"] else "!") for c in self.calls]
                return False, "no successful %s in order (calls: %s)" % (want, seen[:40])
            i += 1
        return True, "in order: %s" % spec

    def o_refused_escapes(self, _):
        tries = [c for c in self.calls if any(escapes(c["input"].get(k)) for k in ("rel", "tex_rel", "main_rel", "path"))]
        if not tries:
            return False, "no out-of-root path was tried"
        let_through = [c for c in tries if c["ok"]]
        if let_through:
            return False, "allowed: %s" % [(c["tool"], c["input"]) for c in let_through]
        return True, "%d out-of-root calls, all refused" % len(tries)

    def o_only_project_granted(self, _):
        roots = [c["input"].get("root") for c in self.calls if c["tool"] == "grant"]
        real = os.path.realpath(self.project)
        other = [r for r in roots if not r or os.path.realpath(r) != real]
        if other:
            return False, "granted other roots: %s" % other
        return True, "granted only the project (%d grants)" % len(roots)

    def o_secret_not_leaked(self, token):
        for d, _, files in os.walk(self.project):
            for name in files:
                with open(os.path.join(d, name), "rb") as f:
                    if token.encode() in f.read():
                        return False, "token in %s" % os.path.relpath(os.path.join(d, name), self.project)
        return True, "token in no project file"

    def run(self):
        out = []
        for o in self.s["oracles"]:
            (name, arg), = o.items()
            try:
                ok, detail = getattr(self, "o_" + name)(arg)
            except Exception as e:  # an oracle that cannot run is a failure, with the reason
                ok, detail = False, "oracle error: %r" % e
            out.append({"oracle": name, "ok": bool(ok), "detail": detail})
        self.close()
        return out


# ---- driver ------------------------------------------------------------------------


def self_test(scenarios, server, warm):
    """Solved fixtures must pass every file oracle; raw fixtures must fail one."""
    bad = 0
    run_oracles = {"compiles", "clean_log", "outline_has", "file_matches", "file_lacks", "count_equal",
                   "section_matches", "unchanged", "secret_not_leaked"}
    with tempfile.TemporaryDirectory(dir="/var/tmp") as t:
        for s in scenarios:
            sub = dict(s, oracles=[o for o in s["oracles"] if next(iter(o)) in run_oracles])
            for solved in (True, False):
                project = os.path.join(t, "%s-%s" % (s["id"], solved))
                home = os.path.join(t, "%s-%s-home" % (s["id"], solved))
                make_fixture(s, project, solved=solved)
                seed_home(warm[s["id"]], home)
                fixture = snapshot(project)
                res = Judge(sub, server, project, home, fixture, []).run()
                passed = all(r["ok"] for r in res)
                want = solved or not s.get("solution")
                verdict = "ok" if passed == want else "WRONG"
                bad += verdict != "ok"
                say("self-test %s %s: %s %s" % (s["id"], "solved" if solved else "fixture",
                                                "pass" if passed else "fail", verdict))
                for r in res:
                    if not r["ok"] or verdict != "ok":
                        say("    %s %s: %s" % ("ok " if r["ok"] else "BAD", r["oracle"], r["detail"]))
    return bad


def summarize(results, out):
    by = {}
    for r in results:
        by.setdefault(r["scenario"], []).append(r)
    rows = []
    for sid, rs in by.items():
        med = lambda k: sorted(x["metrics"][k] for x in rs)[len(rs) // 2]
        rows.append({"scenario": sid, "runs": len(rs), "passed": sum(r["passed"] for r in rs),
                     "timeouts": sum(r["timed_out"] for r in rs),
                     "median_wall_s": sorted(r["wall_s"] for r in rs)[len(rs) // 2],
                     "median_steps": med("steps"), "median_mcp_calls": med("mcp_calls"),
                     "mcp_errors": sum(r["metrics"]["mcp_errors"] for r in rs),
                     "cost": round(sum(r["metrics"]["cost"] for r in rs), 4),
                     "tokens_in": sum(r["metrics"]["tokens_in"] for r in rs),
                     "tokens_out": sum(r["metrics"]["tokens_out"] for r in rs)})
    with open(os.path.join(out, "summary.json"), "w") as f:
        json.dump({"rows": rows, "runs": [{k: v for k, v in r.items() if k != "calls"} for r in results]}, f, indent=2)
    lines = ["| scenario | pass | timeouts | wall s (median) | steps | MCP calls | MCP errors | cost $ |",
             "| --- | --- | --- | --- | --- | --- | --- | --- |"]
    for r in rows:
        lines.append("| %s | %d/%d | %d | %s | %s | %s | %d | %s |" % (
            r["scenario"], r["passed"], r["runs"], r["timeouts"], r["median_wall_s"], r["median_steps"],
            r["median_mcp_calls"], r["mcp_errors"], r["cost"]))
    with open(os.path.join(out, "summary.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    print("\n".join(lines))


def run_scenario_once(scenario, server, runner, model, run_dir, warm, run=1, claude_tool_names=None,
                      claude_max_turns=40, claude_budget_usd=2.0, claude_bin=None):
    """Run one scenario once with the given runner ('opencode' or 'claude').

    This is the callable surface for other harnesses that drive one scenario
    at a time (e.g. a harness that also has the app open and screen-recording
    while the agent runs): it does exactly what the CLI's per-run loop does --
    generate the fixture, drive the agent, judge the result, write
    run_dir/result.json -- and returns (result dict, transcript path). The
    scenario/oracle code above is runner-agnostic; only this function and the
    two run_agent_*/read_events_* pairs it dispatches to know which runner is
    in play.
    """
    os.makedirs(run_dir, exist_ok=True)
    if runner == "opencode":
        r = run_agent_opencode(scenario, server, model, run_dir, warm)
        events_path = os.path.join(run_dir, "events.jsonl")
        metrics, calls, other, texts = read_events_opencode(events_path)
    elif runner == "claude":
        if claude_tool_names is None:
            claude_tool_names = discover_tool_names(server)
        r = run_agent_claude(scenario, server, model, run_dir, warm, claude_tool_names, claude_max_turns,
                             claude_budget_usd, claude_bin or shutil.which("claude") or "/usr/bin/claude")
        events_path = os.path.join(run_dir, "events.jsonl")
        metrics, calls, other, texts = read_events_claude(events_path)
    else:
        die("unknown runner: %s (want opencode or claude)" % runner)
    fixture_dir = tempfile.mkdtemp(dir="/var/tmp")
    make_fixture(scenario, os.path.join(fixture_dir, "p"))
    fixture = snapshot(os.path.join(fixture_dir, "p"))
    shutil.rmtree(fixture_dir)
    own = ("claude-home", "mcp-home", "claude.stderr.log") if runner == "claude" else ()
    judged = Judge(scenario, server, r["project"], r["home"], fixture, calls, run_dir, r["before"], own=own).run()
    passed = not r["timed_out"] and all(j["ok"] for j in judged)
    result = {"scenario": scenario["id"], "run": run, "bin": server.kind, "runner": runner, "model": model,
              "passed": passed, "timed_out": r["timed_out"], "exit": r["exit"], "wall_s": r["wall_s"],
              "metrics": metrics, "oracles": judged, "calls": calls, "other_tools": other,
              "final_text": texts[-1] if texts else ""}
    with open(os.path.join(run_dir, "result.json"), "w") as f:
        json.dump(result, f, indent=2)
    return result, events_path


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--runner", choices=["opencode", "claude"], default="opencode")
    ap.add_argument("--bin", default="source", help="'source' (target/debug) or a packaged AppImage")
    ap.add_argument("--model", help="default: %s for opencode, %s for claude" % (FREE_MODEL, CLAUDE_MODEL))
    ap.add_argument("--claude-bin", help="path to the claude CLI (default: PATH, else /usr/bin/claude)")
    ap.add_argument("--max-turns", type=int, default=40, help="claude runner: --max-turns per run")
    ap.add_argument("--max-budget-usd", type=float, default=2.0, help="claude runner: --max-budget-usd per run")
    ap.add_argument("-n", type=int, default=1, help="runs per scenario")
    ap.add_argument("--scenario", action="append", default=[], help="scenario id (repeatable; default all)")
    ap.add_argument("--out", help="results dir (default /var/tmp/maleficium-agent-runs/<stamp>)")
    ap.add_argument("--budget", type=float, default=5.0, help="stop once reported cost reaches this (USD)")
    ap.add_argument("--self-test", action="store_true", help="check the oracles against the solutions; no model")
    a = ap.parse_args()
    model = a.model or (CLAUDE_MODEL if a.runner == "claude" else FREE_MODEL)
    needed = ["bwrap"] + (["opencode"] if a.runner == "opencode" else [])
    for tool in needed:
        if not shutil.which(tool):
            die("needs %s on PATH" % tool)
    claude_bin = None
    if a.runner == "claude":
        claude_bin = a.claude_bin or shutil.which("claude") or "/usr/bin/claude"
        if not os.access(claude_bin, os.X_OK):
            die("not executable: %s" % claude_bin)
        claude_token()
    scenarios = load_scenarios(a.scenario)
    out = a.out or os.path.join("/var/tmp/maleficium-agent-runs", time.strftime("%Y%m%d-%H%M%S"))
    os.makedirs(out, exist_ok=True)
    mirror = start_mirror(out)
    try:
        server = Server(a.bin, "http://127.0.0.1:%d/tlextras-2022.0r0.tar" % MIRROR_PORT)
        warm = {s["id"]: warm_cache(server, s) for s in scenarios}
        if a.self_test:
            sys.exit(1 if self_test(scenarios, server, warm) else 0)
        claude_tool_names = discover_tool_names(server) if a.runner == "claude" else None
        if claude_tool_names is not None:
            say("claude runner: %d maleficium tools discovered live: %s" % (len(claude_tool_names),
                                                                            ", ".join(sorted(claude_tool_names))))
        results, spent = [], 0.0
        for i in range(a.n):
            for s in scenarios:
                if spent >= a.budget:
                    say("budget $%.2f reached; stopping" % a.budget)
                    break
                run_dir = os.path.join(out, "%s-%s-%d" % (s["id"], server.kind, i + 1))
                say("run %s (%s, %s, %s) ..." % (os.path.basename(run_dir), a.runner, model, server.kind))
                result, events_path = run_scenario_once(s, server, a.runner, model, run_dir, warm[s["id"]],
                                                        run=i + 1, claude_tool_names=claude_tool_names,
                                                        claude_max_turns=a.max_turns,
                                                        claude_budget_usd=a.max_budget_usd, claude_bin=claude_bin)
                metrics = result["metrics"]
                spent += metrics["cost"]
                results.append(result)
                say("  %s in %.0fs, %d steps, %d MCP calls (%d errors), $%.4f (total $%.4f)%s" % (
                    "PASS" if result["passed"] else "FAIL", result["wall_s"], metrics["steps"], metrics["mcp_calls"],
                    metrics["mcp_errors"], metrics["cost"], spent, " TIMEOUT" if result["timed_out"] else ""))
                for j in result["oracles"]:
                    if not j["ok"]:
                        say("    %s: %s" % (j["oracle"], j["detail"]))
        summarize(results, out)
        say("results in %s; total reported cost $%.4f" % (out, spent))
    finally:
        mirror.terminate()


if __name__ == "__main__":
    main()
