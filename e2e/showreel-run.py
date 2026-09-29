#!/usr/bin/env python3
"""showreel-run.py — record an agent writing a document in the open app, for a video.

The agent works live and unassisted through a scripted conversation: one
`claude -p` session, resumed for each beat of the scenario
(e2e/agent-scenarios/showreel/*.json). The harness never edits the
document. It only moves the camera: a director reads the agent's
transcript as it streams and follows it in the app. It pre-positions on
each file the agent writes, holds the changed line while the app reloads
it, jumps the preview to the matching
page after each compile (forward SyncTeX), follows the region the agent
asks the snippet tool about, and ends on a slow scroll through the pages.

Each take is judged afterwards, beat by beat, on a copy of the project as
it stood when that beat ended. A take passes only if every beat does, so a
video can state an honest pass rate over its takes.

Per take, in --out/take-N: screen.mp4 (native size, no cursor),
captions.srt (the scripted requests, timed to the video), events.jsonl (the
agent's stream-json, each line with _ts_ms and _beat), camera.jsonl (every
camera move), timeline.jsonl, app-events.jsonl, the project and pdf per
beat, and result.json.

The project lives at $SHOWREEL_HOME/<scenario project> (default
/tmp/barista/papers/...), so paths on screen read like a person's. The app and
the MCP server share that HOME; the agent's own HOME is a scratch dir inside
it holding only config (auth comes from the setup-token, see agent-run.py).

Usage:
  python3 e2e/showreel-run.py [--scenario coffee] [-n N] [--model M] [--beats N]
                              [--out DIR] [--budget USD] [--no-build]
Env: SHOWREEL_DISPLAY (:96), SHOWREEL_SCREEN (1536x864: upscale to 1080p in
the edit for a larger UI), SHOWREEL_PORT (1424), SHOWREEL_MIRROR_PORT
(18792), SHOWREEL_HOME (/tmp/barista), SHOWREEL_DEBUG=1 (a screenshot per camera
move), CARGO_TARGET_DIR (default
/var/tmp/maleficium-showreel-target).
Manual only: it spends model credits.
"""
import argparse, difflib, importlib.util, json, os, queue, re, shutil, signal, subprocess, sys, threading, time
import harness

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

_spec = importlib.util.spec_from_file_location("agent_run", os.path.join(HERE, "agent-run.py"))
ar = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ar)

DISP = os.environ.get("SHOWREEL_DISPLAY", ":96")
SCREEN = os.environ.get("SHOWREEL_SCREEN", "1536x864")
PORT = int(os.environ.get("SHOWREEL_PORT", "1424"))
MIRROR_PORT = int(os.environ.get("SHOWREEL_MIRROR_PORT", "18792"))
TARGET = os.environ.get("CARGO_TARGET_DIR") or "/var/tmp/maleficium-showreel-target"
APPBIN = os.path.join(TARGET, "debug", "maleficium")
VITE_CACHE = os.environ.get("VITE_CACHE_DIR", "/var/tmp/maleficium-showreel-vite-cache")

HOME = os.environ.get("SHOWREEL_HOME", "/tmp/barista")
# Only a HOME holding this marker is ever wiped.
MARKER = ".showreel-home"
AGENT_HOME = os.path.join(HOME, ".agent")
MCP_CONFIG = os.path.join(HOME, ".agent-mcp.json")
CACHE = "/var/tmp/maleficium-showreel-cache"
# Every package a toy note is likely to reach for, compiled once so takes
# start from a warm engine cache (anything else is fetched through the mirror).
SOAK = r"""\documentclass[11pt]{article}
\usepackage{amsmath,amssymb,graphicx,booktabs,siunitx,xcolor,geometry,caption,float,hyperref}
\usepackage{tikz,pgfplots}
\usetikzlibrary{arrows.meta,decorations.pathmorphing,shapes,positioning,calc}
\pgfplotsset{compat=1.18}
\begin{document}
\title{T}\author{A}\maketitle
\begin{abstract}x\end{abstract}
\section{S}$\frac{dT}{dt}=-k(T-T_a)$ \SI{80}{\celsius}
\begin{tikzpicture}\begin{axis}[width=\textwidth]\addplot{x};\end{axis}\end{tikzpicture}
\begin{tabular}{lr}\toprule a&1\\\bottomrule\end{tabular}
%s
\end{document}
"""
# The agent's terminal: its own display, recorded beside the app.
TERM_DISPLAY = os.environ.get("SHOWREEL_TERM_DISPLAY", ":95")
TERM_SCREEN = os.environ.get("SHOWREEL_TERM_SCREEN", "1280x864")
TERM_COLS, TERM_ROWS, TERM_FONT = 116, 40, 13
TMUX_SOCKET = "maleficium-showreel"
# The camera channel's file, served to the app by the dev server (vite.config.ts).
CAMERA_FILE = os.environ.setdefault("SHOWREEL_CAMERA_FILE", "/var/tmp/maleficium-showreel-camera.jsonl")
# Seconds each camera move holds before the next.
DWELL = 1.2
# SHOWREEL_DEBUG=1: a screenshot after every camera move, to aim the moves.
DEBUG = os.environ.get("SHOWREEL_DEBUG") == "1"


def say(msg):
    print("showreel: " + msg, flush=True)


def die(msg):
    raise SystemExit("showreel: FATAL " + msg)


def now_ms():
    return int(time.time() * 1000)


# ---- the agent --------------------------------------------------------------------


class PrintSession:
    """The fallback agent: `claude -p`, resumed for each beat, streaming to one
    file. Nothing on screen but the app."""

    def __init__(self, server, model, events_path, tool_names, budget_usd, on_event):
        self.server, self.model, self.events_path = server, model, events_path
        self.on_event, self.budget_usd = on_event, budget_usd
        self.allowed = ar.CLAUDE_BUILTIN_TOOLS + ["mcp__maleficium__" + t for t in tool_names]
        self.claude = shutil.which("claude") or "/usr/bin/claude"
        self.id = None
        self.cost = 0.0
        os.makedirs(os.path.join(AGENT_HOME, ".claude"), exist_ok=True, mode=0o700)
        with open(MCP_CONFIG, "w") as f:
            json.dump({"mcpServers": {"maleficium": {"type": "stdio", "command": server.cmd[0],
                                                     "args": server.cmd[1:], "env": dict(server.env, HOME=HOME)}}},
                      f, indent=2)

    def turn(self, beat, prompt, project, timeout):
        argv = [self.claude, "-p", "--output-format", "stream-json", "--verbose",
                "--max-turns", "60", "--max-budget-usd", str(self.budget_usd),
                "--setting-sources", "", "--disable-slash-commands", "--restricted",
                "--tools", ",".join(ar.CLAUDE_BUILTIN_TOOLS),
                "--mcp-config", MCP_CONFIG, "--strict-mcp-config",
                "--allowedTools"] + self.allowed + ["--model", self.model]
        if self.id:
            argv += ["--resume", self.id]
        argv.append(prompt)
        env = dict(os.environ, HOME=AGENT_HOME, CLAUDE_CODE_OAUTH_TOKEN=ar.claude_token())
        p = subprocess.Popen(ar.sandbox(HOME, project, argv), stdout=subprocess.PIPE,
                             stderr=open(os.path.join(os.path.dirname(self.events_path), "claude.stderr.log"), "a"),
                             stdin=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True, env=env)
        result = {}

        def pump():
            with open(self.events_path, "a") as out:
                for line in iter(p.stdout.readline, ""):
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        e = json.loads(line)
                    except ValueError:
                        e = {"_raw": line}
                    if not isinstance(e, dict):
                        e = {"_raw": line}
                    e["_ts_ms"], e["_beat"] = now_ms(), beat
                    out.write(json.dumps(e) + "\n")
                    out.flush()
                    if e.get("type") == "system" and e.get("subtype") == "init" and e.get("session_id"):
                        self.id = e["session_id"]
                    if e.get("type") == "result":
                        result.update(e)
                    try:
                        self.on_event(e)
                    except Exception as ex:  # the camera must never stall the agent's pipe
                        print("showreel: director skipped an event: %s" % ex, flush=True)

        reader = threading.Thread(target=pump, daemon=True)
        reader.start()
        t0 = time.time()
        try:
            code, timed_out = p.wait(timeout=timeout), False
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.wait()
            code, timed_out = None, True
        reader.join(10)
        cost = float(result.get("total_cost_usd") or 0.0)
        self.cost += cost
        return {"exit": code, "timed_out": timed_out, "wall_s": round(time.time() - t0, 1), "cost": cost,
                "is_error": bool(result.get("is_error")), "turns": result.get("num_turns")}


class TmuxSession:
    """The agent on camera: interactive Claude Code in tmux, in an xterm on
    its own display, recorded beside the app. The harness types each beat
    into it as a person would, and a Stop hook marks the end of each turn.
    The transcript is Claude Code's own session log, tailed as it grows."""

    def __init__(self, server, model, events_path, tool_names, on_event, procs, take_dir):
        self.model, self.events_path, self.on_event = model, events_path, on_event
        self.procs, self.take_dir = procs, take_dir
        self.allowed = ar.CLAUDE_BUILTIN_TOOLS + ["mcp__maleficium__" + t for t in tool_names]
        self.claude = shutil.which("claude") or "/usr/bin/claude"
        self.turns = os.path.join(HOME, ".agent-turns")
        self.settings = os.path.join(HOME, ".agent-settings.json")
        self.cost = 0.0
        self.beat = None
        self.tail_halt = threading.Event()
        self.capture = None
        os.makedirs(os.path.join(AGENT_HOME, ".claude"), exist_ok=True, mode=0o700)
        with open(MCP_CONFIG, "w") as f:
            json.dump({"mcpServers": {"maleficium": {"type": "stdio", "command": server.cmd[0],
                                                     "args": server.cmd[1:], "env": dict(server.env, HOME=HOME)}}},
                      f, indent=2)
        with open(self.settings, "w") as f:
            json.dump({"hooks": {"Stop": [{"hooks": [{"type": "command",
                                                      "command": "date +%%s >> %s" % self.turns}]}]}}, f)

    def tmux(self, *args, env=None):
        return subprocess.run(["tmux", "-L", TMUX_SOCKET, *args], env=env, capture_output=True, text=True,
                              timeout=15)

    def start(self, project):
        with open(os.path.join(AGENT_HOME, ".claude.json"), "w") as f:
            json.dump({"hasCompletedOnboarding": True, "theme": "dark",
                       "projects": {project: {"hasTrustDialogAccepted": True,
                                              "hasCompletedProjectOnboarding": True}}}, f)
        argv = ar.sandbox(HOME, project, [
            self.claude, "--setting-sources", "", "--settings", self.settings,
            "--disable-slash-commands", "--restricted", "--tools", ",".join(ar.CLAUDE_BUILTIN_TOOLS),
            "--mcp-config", MCP_CONFIG, "--strict-mcp-config",
            "--allowedTools"] + self.allowed + ["--model", self.model])
        # The token reaches claude through the tmux server's environment: no
        # file, and nothing in a command line.
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "TERM": "xterm-256color", "HOME": AGENT_HOME,
               "LANG": "C.UTF-8", "CLAUDE_CODE_OAUTH_TOKEN": ar.claude_token(),
               # no "Update available!" notice on camera
               "DISABLE_AUTOUPDATER": "1"}
        self.tmux("kill-server")
        r = self.tmux("new-session", "-d", "-s", "agent", "-x", str(TERM_COLS), "-y", str(TERM_ROWS),
                      "-c", project, " ".join(shlex_quote(a) for a in argv), env=env)
        if r.returncode != 0:
            die("tmux did not start: %s" % r.stderr.strip())
        self.tmux("set-option", "-g", "status", "off")
        self.xterm = self.procs.start(
            ["xterm", "-fa", "DejaVu Sans Mono", "-fs", str(TERM_FONT), "-bg", "#1b1b1f", "-fg", "#e6e6e6",
             "-geometry", "%dx%d+0+0" % (TERM_COLS, TERM_ROWS), "-b", "0", "+sb",
             "-e", "tmux", "-L", TMUX_SOCKET, "attach", "-t", "agent"],
            env=dict(os.environ, DISPLAY=TERM_DISPLAY), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        end = time.time() + 30
        while time.time() < end:
            if "❯" in self.tmux("capture-pane", "-p", "-t", "agent").stdout:
                break
            time.sleep(0.5)
        else:
            die("claude never showed its prompt in tmux")
        time.sleep(1.5)
        threading.Thread(target=self.tail, daemon=True).start()

    def start_capture(self):
        self.capture = self.procs.start(
            ["ffmpeg", "-loglevel", "error", "-f", "x11grab", "-draw_mouse", "0", "-video_size", TERM_SCREEN,
             "-framerate", "30", "-i", TERM_DISPLAY, "-c:v", "libx264", "-preset", "veryfast", "-crf", "18",
             "-pix_fmt", "yuv420p", "-movflags", "+frag_keyframe+empty_moov", os.path.join(self.take_dir, "claude.mp4")],
            stdout=subprocess.DEVNULL, stderr=open(os.path.join(self.take_dir, "claude-capture.log"), "w"))

    def tail(self):
        """Copy Claude Code's session log into events.jsonl as it grows, each
        line stamped, and hand every event to the director."""
        path, pos = None, 0
        projects = os.path.join(AGENT_HOME, ".claude", "projects")
        while not self.tail_halt.is_set():
            if path is None:
                logs = [os.path.join(d, n) for d, _, fs in os.walk(projects) for n in fs if n.endswith(".jsonl")]
                path = max(logs, key=os.path.getmtime) if logs else None
            if path:
                with open(path, "rb") as f:
                    f.seek(pos)
                    chunk = f.read()
                # keep a partial last line for the next read
                cut = chunk.rfind(b"\n") + 1
                pos += cut
                if cut:
                    with open(self.events_path, "a") as out:
                        for line in chunk[:cut].decode("utf-8", "replace").splitlines():
                            try:
                                e = json.loads(line)
                            except ValueError:
                                continue
                            e["_ts_ms"], e["_beat"] = now_ms(), self.beat
                            out.write(json.dumps(e) + "\n")
                            try:
                                self.on_event(e)
                            except Exception as ex:
                                print("showreel: director skipped an event: %s" % ex, flush=True)
            time.sleep(0.2)

    def done_turns(self):
        try:
            return sum(1 for _ in open(self.turns))
        except OSError:
            return 0

    def turn(self, beat, prompt, project, timeout):
        self.beat = beat
        before = self.done_turns()
        t0 = time.time()
        # Typed at a readable pace, one short chunk at a time.
        for i in range(0, len(prompt), 3):
            self.tmux("send-keys", "-t", "agent", "-l", "--", prompt[i:i + 3])
            time.sleep(0.03)
        time.sleep(0.6)
        self.tmux("send-keys", "-t", "agent", "Enter")
        end = t0 + timeout
        while time.time() < end and self.done_turns() <= before:
            time.sleep(0.5)
        timed_out = self.done_turns() <= before
        return {"exit": None if timed_out else 0, "timed_out": timed_out, "wall_s": round(time.time() - t0, 1),
                "cost": 0.0, "is_error": False, "turns": None}

    def stop(self):
        self.tail_halt.set()
        if self.capture:
            self.procs.stop(self.capture, sig=signal.SIGINT, grace=20)
        self.tmux("kill-server")


def start_term_display(procs):
    if os.path.exists("/tmp/.X%s-lock" % TERM_DISPLAY.lstrip(":")):
        die("an X server already holds %s (set SHOWREEL_TERM_DISPLAY)" % TERM_DISPLAY)
    w, h = TERM_SCREEN.split("x")
    p = procs.start(["Xvfb", TERM_DISPLAY, "-screen", "0", "%sx%sx24" % (w, h), "-nolisten", "tcp"],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    end = time.time() + 20
    while time.time() < end:
        if subprocess.run(["xdotool", "getdisplaygeometry"], env=dict(os.environ, DISPLAY=TERM_DISPLAY),
                          capture_output=True).returncode == 0:
            return p
        time.sleep(0.3)
    die("Xvfb never answered on %s" % TERM_DISPLAY)


def shlex_quote(a):
    import shlex
    return shlex.quote(a)


# ---- the camera -------------------------------------------------------------------


class Director(threading.Thread):
    """Follows the agent in the app. It reads tool calls and results off the
    transcript and turns them into camera moves, one at a time, sent through
    the app's dev camera channel: it sends no keystrokes, so it can never
    change a document. A write pre-positions the camera while the tool runs,
    then holds the changed line while the app reloads it, so the edit lands
    on screen; holds shorten while the camera is behind, and a compile's
    forward-sync yields to newer edits."""

    def __init__(self, app, project, log_path, active):
        super().__init__(daemon=True)
        self.app, self.project, self.log_path = app, project, log_path
        self.q = queue.Queue()
        self.uses = {}
        self.seen = {}
        # The file the app shows: opening it again raises no event.
        self.current = active
        self.last_line = {}
        self.halt = threading.Event()
        self.shots = 0
        self.seq = 0
        open(CAMERA_FILE, "w").close()  # a fresh app starts at move 1
        self.idle = threading.Event()
        self.idle.set()
        for d, _, files in os.walk(project):
            for n in files:
                if n.endswith((".tex", ".bib")):
                    path = os.path.join(d, n)
                    self.seen[path] = open(path, errors="replace").read()

    def log(self, what, **fields):
        with open(self.log_path, "a") as f:
            f.write(json.dumps(dict(at=now_ms(), what=what, **fields)) + "\n")
        if DEBUG and self.app.win:
            self.shots += 1
            self.app.shot("debug-%03d-%s" % (self.shots, what))

    # Transcript side: called from the session's reader thread.
    def on_event(self, e):
        content = (e.get("message") or {}).get("content")
        if not isinstance(content, list):
            return
        for c in content:
            if not isinstance(c, dict):
                continue
            if c.get("type") == "tool_use":
                name, args = c.get("name", ""), c.get("input") or {}
                self.uses[c.get("id")] = (name, args)
                self.on_tool_use(name, args)
            elif c.get("type") == "tool_result":
                name, args = self.uses.get(c.get("tool_use_id"), ("", {}))
                self.on_result(name, args, c)

    def on_tool_use(self, name, args):
        # Pre-position while the tool runs, so the edit lands on screen.
        if name in ("Edit", "Write", "MultiEdit"):
            path = args.get("file_path") or ""
            if path.startswith(self.project + os.sep) and path.endswith(".tex"):
                self.put_show(os.path.relpath(path, self.project), self.guess_line(name, args, path), "use")

    def guess_line(self, name, args, path):
        if name == "Write":
            return 1
        try:
            lines = open(path, errors="replace").read().splitlines()
        except OSError:
            return 1
        edits = args.get("edits") if name == "MultiEdit" else [args]
        for ed in edits or []:
            old = (ed or {}).get("old_string") or ""
            first = old.splitlines()[0] if old.splitlines() else ""
            if first:
                for i, ln in enumerate(lines):
                    if first in ln:
                        return i + 1
        return self.last_line.get(os.path.relpath(path, self.project), 1)

    def put_show(self, rel, line, trigger):
        if trigger == "result":
            # A finished edit supersedes any queued pre-position of its file.
            with self.q.mutex:
                kept = [m for m in self.q.queue
                        if not (m[0] == "show" and m[1] == rel and len(m) > 3 and m[3] == "use")]
                self.q.queue.clear()
                self.q.queue.extend(kept)
        self.q.put(("show", rel, line, trigger))

    def dwell(self):
        # Behind: shorten the hold so each edit still gets screen time.
        return 0.6 if self.q.qsize() > 2 else DWELL

    def on_result(self, name, args, c):
        if c.get("is_error"):
            return
        if name in ("Edit", "Write", "MultiEdit"):
            path = args.get("file_path") or ""
            if path.startswith(self.project + os.sep) and path.endswith(".tex"):
                line = self.changed_line(path)
                self.put_show(os.path.relpath(path, self.project), line, "result")
        elif name == "mcp__maleficium__compile_poll":
            if '"status":"success"' in json.dumps(c.get("content")).replace(" ", "").replace('\\"', '"'):
                self.q.put(("sync", now_ms()))
        elif name == "mcp__maleficium__snippet":
            text = "".join(b.get("text", "") for b in c.get("content") or [] if isinstance(b, dict))
            try:
                rec = json.loads(text[text.index("{"):]) if "{" in text else {}
            except ValueError:
                rec = {}
            src = rec.get("source") or {}
            if src.get("rel") and src.get("line"):
                self.q.put(("snippet", src["rel"], src["line"], rec.get("page")))
            elif rec.get("page"):
                self.q.put(("page", rec["page"]))

    def changed_line(self, path):
        try:
            new = open(path, errors="replace").read()
        except OSError:
            return 1
        old = self.seen.get(path, "")
        self.seen[path] = new
        sm = difflib.SequenceMatcher(None, old.splitlines(), new.splitlines(), autojunk=False)
        for tag, _, _, j1, _ in sm.get_opcodes():
            if tag != "equal":
                return j1 + 1
        return 1

    # Camera side: every queued move runs, so every edit gets screen time.
    def run(self):
        while not self.halt.is_set():
            try:
                move = self.q.get(timeout=0.3)
            except queue.Empty:
                self.idle.set()
                continue
            self.idle.clear()
            try:
                getattr(self, "do_" + move[0])(*move[1:])
            except Exception as ex:  # a missed move must not end the take
                self.log("error", move=list(move), error=str(ex))

    def wait_idle(self, timeout=30):
        end = time.time() + timeout
        while time.time() < end:
            if self.q.empty() and self.idle.wait(0.5) and self.q.empty():
                return
        self.log("idle-timeout")

    # Camera moves go to the app through its dev-build camera channel
    # (src/lib/devCamera.ts): numbered moves appended to CAMERA_FILE, run by
    # the app's own handlers. No keystrokes, so no move can change a
    # document. The only input device the camera uses is the mouse wheel.

    def send(self, op, **fields):
        self.seq += 1
        with open(CAMERA_FILE, "a") as f:
            f.write(json.dumps(dict(seq=self.seq, op=op, **fields)) + "\n")
        time.sleep(0.4)  # the app polls every 250 ms

    def open_file(self, rel):
        if self.current == rel:
            return True
        path = os.path.join(self.project, rel)
        m = now_ms()
        self.send("open", rel=rel)
        ok = self.app.log.wait("file.open", m - 1, 5, lambda ev: ev.get("path") == path) or \
            self.app.log.wait("file.switch", m - 1, 1, lambda ev: ev.get("path") == path)
        if ok:
            self.current = rel
        else:
            self.log("open-failed", rel=rel)
        return bool(ok)

    def do_show(self, rel, line, trigger="result"):
        if self.open_file(rel):
            self.send("line", line=line)
            self.last_line[rel] = line
            if trigger == "result":
                # Hold while the app reloads the file, so the edit lands on screen.
                self.app.log.wait("file.reload", now_ms() - 2000, 4,
                                  lambda ev, p=os.path.join(self.project, rel): ev.get("path") == p)
            self.log("show", rel=rel, line=line, trigger=trigger)
            time.sleep(self.dwell())

    def do_sync(self, since):
        # The preview reloads on its own after an outside compile; jump once it has.
        end = time.time() + 12
        while time.time() < end:
            if self.app.log.find("preview.page-render", since - 1):
                break
            if any(m[0] in ("show", "snippet") for m in list(self.q.queue)):
                break  # newer edits first; the next compile queues a fresh sync
            time.sleep(0.5)
        if self.current:
            self.send("command", id="tools.forward-sync")
            self.log("sync", rel=self.current, line=self.last_line.get(self.current))
            time.sleep(self.dwell())

    def do_snippet(self, rel, line, page):
        if self.open_file(rel):
            self.send("line", line=line)
            self.last_line[rel] = line
            self.send("command", id="tools.forward-sync")
            self.log("snippet", rel=rel, line=line, page=page)
            time.sleep(self.dwell() + 1.0)

    def do_page(self, page):
        self.send("page", page=page)
        self.log("page", page=page)
        time.sleep(self.dwell() + 0.5)

    def do_walk(self, pages):
        """Fit the page width, then scroll through the document slowly."""
        self.send("command", id="view.zoom-fit-width")
        self.send("page", page=1)
        self.log("walk", pages=pages)
        w, h = (int(x) for x in SCREEN.split("x"))
        harness.xdo(DISP, "mousemove", "--window", self.app.win, str(int(w * 0.83)), str(h // 2))
        for _ in range(max(1, pages) * 12):
            harness.xdo(DISP, "click", "5")
            time.sleep(0.15)
        time.sleep(2)


# ---- judging ----------------------------------------------------------------------


def tex_texts(project):
    out = {}
    for d, _, files in os.walk(project):
        for n in sorted(files):
            if n.endswith(".tex"):
                p = os.path.join(d, n)
                out[os.path.relpath(p, project)] = open(p, errors="replace").read()
    return out


def envs(text, name):
    return re.findall(r"\\begin\{%s\}.*?\\end\{%s\}" % (name, name), text, re.S)


def colspec_width(text):
    m = re.search(r"\\begin\{tabular\}\s*(?:\[[^\]]*\])?\s*\{", text)
    if not m:
        return 0
    # The colspec nests @{} gutters and [] options: read it balanced.
    i, depth, buf = m.end(), 1, []
    while i < len(text) and depth:
        ch = text[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if not depth:
                break
        buf.append(ch)
        i += 1
    spec = "".join(buf)
    spec = re.sub(r"@\{[^{}]*\}", "", spec)  # gutters hold no columns
    spec = re.sub(r"\[[^\]]*\]", "", spec)  # options hold no columns
    spec = re.sub(r"\*\{(\d+)\}\{([^{}]*)\}", lambda m: m.group(2) * int(m.group(1)), spec)
    return len(re.findall(r"[lcrpXS]", spec))


def judge_beat(mcp, snap, beat, prev_texts, idx):
    """Compile the beat's snapshot on its own root and check its oracles."""
    texts = tex_texts(snap)
    alltext = "\n".join(texts.values())
    prev = "\n".join(prev_texts.values())
    rid = "beat%d" % idx
    ok, err = mcp.call("grant", {"root_id": rid, "root": snap})
    if not ok:
        return [("grant", False, str(err))], None
    rec = mcp.compile(rid, "main.tex")
    compiled = rec.get("status") == "success"
    pages = None
    if compiled:
        ok, s = mcp.call("snippet", {"root_id": rid, "main_rel": "main.tex", "page": 1})
        pages = s.get("pages") if ok else None
    res = []
    for o in beat["oracles"]:
        kind, args = o[0], o[1:]
        if kind == "compiles":
            res.append(("compiles", compiled, rec.get("status")))
        elif kind == "pages_at_least":
            res.append(("pages>=%d" % args[0], bool(pages and pages >= args[0]), "pages %s" % pages))
        elif kind == "cites":
            miss = [k for k in args[0] if not re.search(r"\\\w*cite\w*\*?(\[[^\]]*\])*\{[^}]*\b%s\b" % re.escape(k), alltext)]
            res.append(("cites", not miss, "missing %s" % miss if miss else "all cited"))
        elif kind == "matches":
            res.append(("matches %s" % args[0], bool(re.search(args[0], alltext)), ""))
        elif kind == "count_at_least":
            n = len(re.findall(args[0], alltext))
            res.append(("count %s>=%d" % (args[0], args[1]), n >= args[1], "found %d" % n))
        elif kind == "more_than_before":
            a, b = len(re.findall(args[0], prev)), len(re.findall(args[0], alltext))
            res.append(("more %s" % args[0], b > a, "%d -> %d" % (a, b)))
        elif kind == "table_columns_grew":
            a, b = colspec_width(prev), colspec_width(alltext)
            res.append(("table columns grew", b > a, "%d -> %d" % (a, b)))
        elif kind == "tikz_beyond_axis":
            n = sum(1 for t in envs(alltext, "tikzpicture") if "\\begin{axis}" not in t)
            res.append(("a tikz diagram besides the plot", n >= 1, "%d" % n))
        elif kind == "plot_full_width":
            figs = [f for f in envs(alltext, "figure\\*?") + envs(alltext, "figure") if "\\begin{axis}" in f]
            wide = any(re.search(r"width\s*=\s*(1(\.0+)?|0?\.9\d*)?\s*\\(text|line)width", f) for f in figs)
            res.append(("plot full width", wide, "%d plot figures" % len(figs)))
        elif kind == "plot_at_top_of_page":
            # The plot's figure, found by its caption: SyncTeX places a
            # caption where it is typeset, but not the lines of a tikzpicture.
            where = None
            for rel, t in texts.items():
                for fig in re.finditer(r"\\begin\{figure\*?\}.*?\\end\{figure\*?\}", t, re.S):
                    body = fig.group(0)
                    cap = body.find("\\caption")
                    if "\\begin{axis}" in body and cap >= 0:
                        where = (rel, t[:fig.start() + cap].count("\n") + 1)
                        break
                if where:
                    break
            placed = None
            if compiled and where:
                ok, s = mcp.call("snippet", {"root_id": rid, "main_rel": "main.tex", "tex_rel": where[0],
                                             "line": where[1]})
                placed = s if ok else {"error": s}
            # A top float's caption sits in the upper half of the page (792 pt).
            good = bool(placed and placed.get("page") == args[0] and (placed.get("region") or {}).get("y", 999) < 450)
            res.append(("plot at top of page %d" % args[0], good,
                        "page %s y %s" % ((placed or {}).get("page"), ((placed or {}).get("region") or {}).get("y"))))
        else:
            res.append((kind, False, "unknown oracle"))
    return res, pages


# ---- a take -----------------------------------------------------------------------


def srt_time(ms):
    ms = max(0, int(ms))
    return "%02d:%02d:%02d,%03d" % (ms // 3600000, ms // 60000 % 60, ms // 1000 % 60, ms % 1000)


def warm(server):
    home = os.path.join(CACHE, "home")
    if os.path.isdir(os.path.join(home, ".cache", ar.IDENT, "maleficium-tectonic")):
        return home
    os.makedirs(home, exist_ok=True)
    proj = os.path.join(CACHE, "soak")
    os.makedirs(proj, exist_ok=True)
    with open(os.path.join(proj, "main.tex"), "w") as f:
        f.write(SOAK % ar.FONT_SOAK)
    mcp = ar.Mcp(server, home)
    try:
        mcp.call("grant", {"root_id": "soak", "root": proj})
        rec = mcp.compile("soak", "main.tex")
    finally:
        mcp.close()
    if rec.get("status") != "success":
        shutil.rmtree(home)
        die("warm compile failed: %s" % str(rec.get("log") or rec)[:600])
    say("engine cache warm (%s)" % home)
    return home


def one_take(s, server, warm_home, model, take_dir, procs, preset, bundle_url, tool_names, budget, max_beats,
             agent):
    os.makedirs(take_dir)
    if os.path.exists(HOME):
        if not os.path.exists(os.path.join(HOME, MARKER)):
            die("%s exists and is not a showreel home; set SHOWREEL_HOME" % HOME)
        shutil.rmtree(HOME)
    project = os.path.join(HOME, s["project"])
    os.makedirs(project)
    open(os.path.join(HOME, MARKER), "w").close()
    for rel, text in s["files"].items():
        with open(os.path.join(project, rel), "w") as f:
            f.write(text)
    ar.seed_home(warm_home, HOME)
    tl = harness.Timeline(os.path.join(take_dir, "timeline.jsonl"))
    tl.mark("take.start", scenario=s["id"], model=model, project=project)
    app = harness.App(procs, take_dir, HOME, preset, bundle_url, tl, APPBIN, ROOT, DISP, SCREEN, ar.IDENT)
    director = Director(app, project, os.path.join(take_dir, "camera.jsonl"), s["main"])
    events_path = os.path.join(take_dir, "events.jsonl")
    if agent == "tmux":
        session = TmuxSession(server, model, events_path, tool_names, director.on_event, procs, take_dir)
    else:
        session = PrintSession(server, model, events_path, tool_names, budget, director.on_event)
    capture, beats = None, []
    try:
        app.launch(project)
        app.place()
        app.open_project()
        if not app.log.wait("compile.warm-skipped", app.launched - 1, 30):
            say("no warm-skipped event; continuing")
        director.send("command", id="view.toggle-log")  # hide the log pane (it shows full paths)
        if agent == "tmux":
            session.start(project)
        director.start()
        capture = procs.start(["ffmpeg", "-loglevel", "error", "-f", "x11grab", "-draw_mouse", "0",
                               "-video_size", SCREEN, "-framerate", "30", "-i", DISP,
                               "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+frag_keyframe+empty_moov",
                               os.path.join(take_dir, "screen.mp4")],
                              stdout=subprocess.DEVNULL, stderr=open(os.path.join(take_dir, "capture.log"), "w"))
        cap0 = tl.mark("capture.start", video="screen.mp4")
        if agent == "tmux":
            session.start_capture()
            tl.mark("capture.start", video="claude.mp4")
        time.sleep(2)
        for i, beat in enumerate(s["beats"][:max_beats]):
            prompt = beat["prompt"]
            full = (s["preamble"].replace("{project}", project) + "\n\n" + prompt) if i == 0 else prompt
            t0 = tl.mark("beat.start", beat=beat["id"], caption=prompt)
            say("  beat %d/%d %s ..." % (i + 1, len(s["beats"]), beat["id"]))
            r = session.turn(beat["id"], full, project, s["timeout_s"])
            director.wait_idle()
            time.sleep(2)
            snap = os.path.join(take_dir, "beats", "%d-%s" % (i + 1, beat["id"]))
            shutil.copytree(project, snap)
            t1 = tl.mark("beat.end", beat=beat["id"], **r)
            beats.append(dict(r, id=beat["id"], prompt=prompt, start_ms=t0, end_ms=t1, snapshot=snap))
            say("    %s in %.0fs, $%.3f" % ("done" if r["exit"] == 0 else "exit %s" % r["exit"], r["wall_s"], r["cost"]))
            if r["timed_out"] or r["exit"] != 0:
                break
        final_pages = None
        ok_stamp = ar.pdf_stamp(server, HOME, project, s["main"])
        if ok_stamp:
            m = tl.mark("walk.start")
            mcp = ar.Mcp(server, HOME)
            try:
                mcp.call("grant", {"root_id": "walk", "root": project})
                ok, sn = mcp.call("snippet", {"root_id": "walk", "main_rel": s["main"], "page": 1})
                final_pages = sn.get("pages") if ok else None
            finally:
                mcp.close()
            director.q.put(("walk", final_pages or 3))
            director.wait_idle(120)
            tl.mark("walk.end", since=m)
        time.sleep(2)
        shutil.copy(app.log.path, os.path.join(take_dir, "app-events.jsonl"))
    finally:
        director.halt.set()
        if agent == "tmux":
            session.stop()
            tl.mark("capture.stop", video="claude.mp4")
        if capture:
            procs.stop(capture, sig=signal.SIGINT, grace=20)
            tl.mark("capture.stop")
        app.stop()

    with open(os.path.join(take_dir, "captions.srt"), "w") as f:
        for n, b in enumerate(beats):
            start = b["start_ms"] - cap0
            f.write("%d\n%s --> %s\n%s\n\n" % (n + 1, srt_time(start), srt_time(start + 6000), b["prompt"]))

    # The take's own engine cache: it holds whatever the agent fetched.
    judge_home = os.path.join(take_dir, "judge-home")
    ar.seed_home(HOME, judge_home)
    mcp = ar.Mcp(server, judge_home)
    prev = {rel: t for rel, t in s["files"].items() if rel.endswith(".tex")}
    try:
        for i, b in enumerate(beats):
            oracles, pages = judge_beat(mcp, b["snapshot"], s["beats"][i], prev, i + 1)
            b["oracles"] = [{"oracle": n, "ok": bool(ok), "detail": d} for n, ok, d in oracles]
            b["pages"] = pages
            b["passed"] = all(o["ok"] for o in b["oracles"]) and b["exit"] == 0 and not b["timed_out"]
            prev = tex_texts(b["snapshot"])
    finally:
        mcp.close()
    shutil.rmtree(judge_home, ignore_errors=True)
    passed = len(beats) == min(len(s["beats"]), max_beats) and all(b["passed"] for b in beats)
    tl.mark("take.end", passed=passed)
    return {"scenario": s["id"], "model": model, "passed": passed, "cost": round(session.cost, 4),
            "beats": beats, "timeline": tl.marks}


def camera_test(s, server, warm_home, out, procs, preset, bundle_url, fixture):
    """No agent: the camera's moves on a finished paper, a screenshot after each."""
    global DEBUG
    DEBUG = True
    if os.path.exists(HOME):
        if not os.path.exists(os.path.join(HOME, MARKER)):
            die("%s exists and is not a showreel home" % HOME)
        shutil.rmtree(HOME)
    project = os.path.join(HOME, s["project"])
    shutil.copytree(fixture, project)
    open(os.path.join(HOME, MARKER), "w").close()
    ar.seed_home(warm_home, HOME)
    tl = harness.Timeline(os.path.join(out, "timeline.jsonl"))
    app = harness.App(procs, out, HOME, preset, bundle_url, tl, APPBIN, ROOT, DISP, SCREEN, ar.IDENT)
    d = Director(app, project, os.path.join(out, "camera.jsonl"), s["main"])
    try:
        app.launch(project)
        app.place()
        app.open_project()
        d.send("command", id="view.toggle-log")
        d.log("start")
        mcp = ar.Mcp(server, HOME)
        try:
            mcp.call("grant", {"root_id": "t", "root": project})
            m = now_ms()
            ok, job = mcp.call("compile_run", {"root_id": "t", "rel": s["main"], "networked": True})
            rec = {"status": "running"}
            while ok and rec.get("status") == "running":
                ok, rec = mcp.call("compile_poll", {"job_id": job["job_id"], "wait_ms": 30000})
            say("camera test compile: %s" % (rec or {}).get("status"))
        finally:
            mcp.close()
        d.do_sync(m)
        lines = open(os.path.join(project, s["main"])).read().count("\n")
        d.do_show(s["main"], max(1, lines * 2 // 3))
        d.do_sync(now_ms())
        d.do_page(1)
        d.do_show("refs.bib", 3) if os.path.exists(os.path.join(project, "refs.bib")) else None
        d.do_snippet(s["main"], max(1, lines // 3), None)
        d.do_walk(3)
        d.log("end")
    finally:
        app.stop()
    say("camera test shots in %s" % out)


def preview(take_dir, speed):
    """A rough cut to judge a take by: the app and Claude Code side by side,
    each beat's request captioned for the whole beat, sped up. Not the edit:
    the raw videos, timeline and captions stay the source for that."""
    marks = [json.loads(l) for l in open(os.path.join(take_dir, "timeline.jsonl"))]
    starts = {m.get("video"): m["at"] for m in marks if m["what"] == "capture.start"}
    app0, term0 = starts.get("screen.mp4"), starts.get("claude.mp4")
    if app0 is None or term0 is None:
        die("%s has no app and terminal recordings to stitch" % take_dir)
    t0 = max(app0, term0)
    beats = [m for m in marks if m["what"] in ("beat.start", "beat.end")]
    srt = os.path.join(take_dir, "preview.srt")
    with open(srt, "w") as f:
        n = 0
        for m in beats:
            if m["what"] != "beat.start":
                continue
            end = next((e["at"] for e in beats if e["what"] == "beat.end" and e["beat"] == m["beat"]), m["at"] + 6000)
            n += 1
            # Captions are timed on the output clock: after the sync trim, sped up.
            f.write("%d\n%s --> %s\n%s\n\n" % (n, srt_time((m["at"] - t0) / speed),
                                                 srt_time((end - t0) / speed), m["caption"]))
    out = os.path.join(take_dir, "preview.mp4")
    style = "FontName=DejaVu Sans,FontSize=13,Outline=1,Shadow=0,MarginV=18,BackColour=&H80000000,BorderStyle=4"
    fc = ("[0:v]trim=start=%.3f,setpts=(PTS-STARTPTS)/%g,scale=-2:864[a];"
          "[1:v]trim=start=%.3f,setpts=(PTS-STARTPTS)/%g,scale=-2:864[t];"
          "[a][t]hstack=inputs=2,scale=1920:-2,subtitles=%s:force_style='%s'[v]"
          % ((t0 - app0) / 1000, speed, (t0 - term0) / 1000, speed, srt, style))
    r = subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", os.path.join(take_dir, "screen.mp4"),
                        "-i", os.path.join(take_dir, "claude.mp4"), "-filter_complex", fc, "-map", "[v]",
                        "-r", "30", "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-pix_fmt", "yuv420p",
                        "-shortest", out], capture_output=True, text=True)
    if r.returncode != 0:
        die("preview failed: %s" % r.stderr.strip()[-600:])
    say("preview (%gx): %s" % (speed, out))


def suggest_shots(take_dir):
    """Candidate shots from a take's shared-clock logs, so the cut shows each
    edit landing: per beat, the request typed, the first finished edit, and
    the first compile payoff after it, plus the final walk. Times are seconds
    from the screen recording's start, matching the per-take edit scripts.
    Prints a SHOTS list; paste into the edit and trim."""
    marks = [json.loads(l) for l in open(os.path.join(take_dir, "timeline.jsonl"))]
    starts = {m.get("video"): m["at"] for m in marks if m["what"] == "capture.start"}
    t0 = starts.get("screen.mp4")
    if t0 is None:
        die("%s has no screen recording to time against" % take_dir)
    beats = [m for m in marks if m["what"] == "beat.start"]
    ends = {m.get("beat"): m["at"] for m in marks if m["what"] == "beat.end"}
    moves = [json.loads(l) for l in open(os.path.join(take_dir, "camera.jsonl"))]
    shots = []
    for i, b in enumerate(beats):
        b0, b1 = b["at"], ends.get(b["beat"], b["at"])
        req = (b0 - t0) / 1000
        edits = [m for m in moves if m.get("what") == "show" and m.get("trigger", "result") == "result"
                 and b0 <= m["at"] <= b1]
        pays = [m for m in moves if m.get("what") == "sync" and b0 <= m["at"] <= b1]
        shots.append((round(req, 1), 6, 2, "full", "req", i,
                      "# %s: request typed" % b.get("beat")))
        if edits:
            e = edits[0]
            shots.append((round((e["at"] - t0) / 1000 - 1, 1), 6, 2, "full", "work", i,
                          "# %s: edit lands %s:%s" % (b.get("beat"), e.get("rel"), e.get("line"))))
            after = [m for m in pays if m["at"] >= e["at"]]
            if after:
                p = after[0]
                shots.append((round((p["at"] - t0) / 1000, 1), 6, 2, "zoom", "work", i,
                              "# %s: compiled payoff" % b.get("beat")))
        elif pays:
            p = pays[0]
            shots.append((round((p["at"] - t0) / 1000, 1), 6, 2, "zoom", "work", i,
                          "# %s: compiled payoff (no edit move logged)" % b.get("beat")))
    walks = [m for m in moves if m.get("what") == "walk"]
    if walks:
        shots.append((round((walks[0]["at"] - t0) / 1000, 1), 15, 2.5, "zoom", "work", len(beats) - 1,
                      "# final walk"))
    print("SHOTS = [")
    for s in shots:
        print("    (%s, %s, %s, %r, %r, %s),  %s" % (s[0], s[1], s[2], s[3], s[4], s[5], s[6]))
    print("]")
    print("# every compiled payoff per beat, for swapping a later one in:")
    for i, b in enumerate(beats):
        b0, b1 = b["at"], ends.get(b["beat"], b["at"])
        pays = [m for m in moves if m.get("what") == "sync" and b0 <= m["at"] <= b1]
        for p in pays:
            print("#   beat %s +%.1fs zoom %s:%s" % (b.get("beat"), (p["at"] - t0) / 1000,
                                                     p.get("rel"), p.get("line")))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--scenario", default="coffee")
    ap.add_argument("-n", type=int, default=1, help="takes")
    ap.add_argument("--model", default=ar.CLAUDE_MODEL)
    ap.add_argument("--out", help="default /var/tmp/maleficium-showreel/<stamp>")
    ap.add_argument("--budget", type=float, default=10.0, help="stop starting takes once spend reaches this")
    ap.add_argument("--beat-budget", type=float, default=3.0, help="claude --max-budget-usd per beat")
    ap.add_argument("--beats", type=int, default=99, help="stop after this many beats (rehearsals)")
    ap.add_argument("--agent", default="tmux", choices=["tmux", "print"],
                    help="tmux: interactive Claude Code on camera (default); print: claude -p, off camera")
    ap.add_argument("--no-build", action="store_true")
    ap.add_argument("--preview", metavar="TAKE_DIR",
                    help="stitch a take's two recordings side by side with captions, sped up, and exit")
    ap.add_argument("--suggest-shots", metavar="TAKE_DIR",
                    help="print candidate edit shots from a take's logs and exit")
    ap.add_argument("--speed", type=float, default=6.0, help="speed-up for --preview")
    ap.add_argument("--camera-test", metavar="PROJECT",
                    help="no agent: run the camera's moves on a finished project, with screenshots")
    a = ap.parse_args()
    if a.preview:
        return preview(a.preview, a.speed)
    if a.suggest_shots:
        return suggest_shots(a.suggest_shots)
    for tool in ["Xvfb", "xdotool", "import", "bwrap", "curl", "ffmpeg", "claude"] + \
            (["tmux", "xterm"] if a.agent == "tmux" else []):
        if not shutil.which(tool):
            die("needs %s on PATH" % tool)
    if not a.camera_test:
        ar.claude_token()
    with open(os.path.join(HERE, "agent-scenarios", "showreel", a.scenario + ".json")) as f:
        s = json.load(f)
    out = a.out or os.path.join("/var/tmp/maleficium-showreel", time.strftime("%Y%m%d-%H%M%S"))
    os.makedirs(out, exist_ok=True)
    procs = harness.Procs()
    lock = harness.take_display_lock(DISP)

    def on_signal(sig, _):
        raise SystemExit("showreel: stopped by signal %d" % sig)
    signal.signal(signal.SIGTERM, on_signal)
    results, spent = [], 0.0
    try:
        if not a.no_build:
            harness.build_app(out, ROOT, TARGET, PORT, say)
        if not os.access(APPBIN, os.X_OK):
            die("no app build at %s" % APPBIN)
        harness.start_mirror(procs, out, HERE, MIRROR_PORT, ar.MIRROR_CACHE)
        bundle_url = "http://127.0.0.1:%d/tlextras-2022.0r0.tar" % MIRROR_PORT
        server = harness.make_server(APPBIN, bundle_url, "showreel", ar)
        warm_home = warm(server)
        tool_names = ar.discover_tool_names(server)
        preset = os.path.join(out, ".preset")
        open(preset, "w").close()
        harness.start_vite(procs, out, ROOT, preset, PORT, VITE_CACHE, say)
        harness.start_xvfb(procs, DISP, SCREEN)
        if a.agent == "tmux" and not a.camera_test:
            start_term_display(procs)
        say("display %s (%s), out %s" % (DISP, SCREEN, out))
        if a.camera_test:
            camera_test(s, server, warm_home, out, procs, preset, bundle_url, a.camera_test)
            return
        for i in range(a.n):
            if spent >= a.budget:
                say("budget $%.2f reached; stopping" % a.budget)
                break
            take_dir = os.path.join(out, "take-%d" % (i + 1))
            say("take %d (%s) ..." % (i + 1, a.model))
            res = one_take(s, server, warm_home, a.model, take_dir, procs, preset, bundle_url, tool_names,
                           a.beat_budget, a.beats, a.agent)
            spent += res["cost"]
            with open(os.path.join(take_dir, "result.json"), "w") as f:
                json.dump(res, f, indent=2)
            results.append(res)
            say("  %s, $%.3f (total $%.3f)" % ("PASS" if res["passed"] else "FAIL", res["cost"], spent))
            for b in res["beats"]:
                say("    beat %s: %s" % (b["id"], "ok" if b["passed"] else "FAIL"))
                for o in b["oracles"]:
                    if not o["ok"]:
                        say("      BAD %s: %s" % (o["oracle"], o["detail"]))
        lines = ["| take | pass | beats passed | cost $ |", "| --- | --- | --- | --- |"]
        for i, r in enumerate(results):
            lines.append("| %d | %s | %d/%d | %.3f |" % (i + 1, "PASS" if r["passed"] else "FAIL",
                                                        sum(b["passed"] for b in r["beats"]), len(s["beats"]),
                                                        r["cost"]))
        with open(os.path.join(out, "summary.md"), "w") as f:
            f.write("\n".join(lines) + "\n")
        print("\n".join(lines))
        say("results in %s; %d/%d takes passed; total $%.3f" % (out, sum(r["passed"] for r in results),
                                                                  len(results), spent))
    finally:
        procs.stop_all()
        shutil.rmtree(lock, ignore_errors=True)
    sys.exit(0 if results and all(r["passed"] for r in results) else 1)


if __name__ == "__main__":
    main()
