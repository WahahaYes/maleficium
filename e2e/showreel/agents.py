#!/usr/bin/env python3
"""agents.py — the agents behind the showreel camera, behind one interface.

An AgentSession works a beat at a time: start() opens the session on the
project, turn() runs one scripted request, stop() closes it down. Every
turn streams its transcript to events.jsonl stamped with _ts_ms and _beat,
and hands each transcript event to the director as a Claude-shaped
message envelope (tool_use / tool_result content blocks), whatever runner
produced it — the director never learns which agent it follows.

ClaudePrint is `claude -p`, resumed for each beat, off camera. TmuxClaude
is interactive Claude Code in tmux in an xterm on its own display,
recorded beside the app (best-effort: it tails Claude Code's own session
log). OpencodePrint is headless `opencode run`, one session resumed for
each beat; the app recording is the video, the transcript renders as the
terminal pane in post.
"""
import json
import os
import shlex
import shutil
import signal
import subprocess
import threading
import time
import uuid

from harness import now_ms, xdo


class AgentSession:
    def __init__(self, ar, server, model, events_path, tool_names, on_event, agent_home, mcp_config, home):
        self.ar, self.server, self.model = ar, server, model
        self.events_path, self.tool_names, self.on_event = events_path, tool_names, on_event
        self.agent_home, self.mcp_config, self.home = agent_home, mcp_config, home
        self.cost = 0.0

    def write_mcp_config(self):
        with open(self.mcp_config, "w") as f:
            json.dump({"mcpServers": {"maleficium": {"type": "stdio", "command": self.server.cmd[0],
                                                      "args": self.server.cmd[1:],
                                                      "env": dict(self.server.env, HOME=self.home)}}},
                      f, indent=2)

    def emit(self, e, beat):
        e["_ts_ms"], e["_beat"] = now_ms(), beat
        with open(self.events_path, "a") as out:
            out.write(json.dumps(e) + "\n")
        try:
            self.on_event(e)
        except Exception as ex:  # the camera must never stall the agent's pipe
            print("showreel: director skipped an event: %s" % ex, flush=True)

    def start(self, project):
        pass

    def turn(self, beat, prompt, project, timeout):
        raise NotImplementedError

    def stop(self):
        pass


class ClaudePrint(AgentSession):
    """The fallback agent: `claude -p`, resumed for each beat, streaming to one
    file. Nothing on screen but the app."""

    def __init__(self, *args, budget_usd=3.0):
        super().__init__(*args)
        self.budget_usd = budget_usd
        self.allowed = self.ar.CLAUDE_BUILTIN_TOOLS + ["mcp__maleficium__" + t for t in self.tool_names]
        self.claude = shutil.which("claude") or "/usr/bin/claude"
        self.id = None
        os.makedirs(os.path.join(self.agent_home, ".claude"), exist_ok=True, mode=0o700)
        self.write_mcp_config()

    def turn(self, beat, prompt, project, timeout):
        argv = [self.claude, "-p", "--output-format", "stream-json", "--verbose",
                "--max-turns", "60", "--max-budget-usd", str(self.budget_usd),
                "--setting-sources", "", "--disable-slash-commands", "--restricted",
                "--tools", ",".join(self.ar.CLAUDE_BUILTIN_TOOLS),
                "--mcp-config", self.mcp_config, "--strict-mcp-config",
                "--allowedTools"] + self.allowed + ["--model", self.model]
        if self.id:
            argv += ["--resume", self.id]
        argv.append(prompt)
        env = dict(os.environ, HOME=self.agent_home, CLAUDE_CODE_OAUTH_TOKEN=self.ar.claude_token())
        p = subprocess.Popen(self.ar.sandbox(self.home, project, argv), stdout=subprocess.PIPE,
                             stderr=open(os.path.join(os.path.dirname(self.events_path), "claude.stderr.log"), "a"),
                             stdin=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True, env=env)
        result = {}

        def pump():
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
                if e.get("type") == "system" and e.get("subtype") == "init" and e.get("session_id"):
                    self.id = e["session_id"]
                if e.get("type") == "result":
                    result.update(e)
                self.emit(e, beat)

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


# The agent's terminal: its own display, recorded beside the app.
TERM_DISPLAY = os.environ.get("SHOWREEL_TERM_DISPLAY", ":95")
TERM_SCREEN = os.environ.get("SHOWREEL_TERM_SCREEN", "1280x864")
TERM_COLS, TERM_ROWS, TERM_FONT = 116, 40, 13
TMUX_SOCKET = "maleficium-showreel"


def start_term_display(procs):
    if os.path.exists("/tmp/.X%s-lock" % TERM_DISPLAY.lstrip(":")):
        raise SystemExit("an X server already holds %s (set SHOWREEL_TERM_DISPLAY)" % TERM_DISPLAY)
    w, h = TERM_SCREEN.split("x")
    p = procs.start(["Xvfb", TERM_DISPLAY, "-screen", "0", "%sx%sx24" % (w, h), "-nolisten", "tcp"],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    end = time.time() + 20
    while time.time() < end:
        if subprocess.run(["xdotool", "getdisplaygeometry"], env=dict(os.environ, DISPLAY=TERM_DISPLAY),
                          capture_output=True).returncode == 0:
            return p
        time.sleep(0.3)
    raise SystemExit("Xvfb never answered on %s" % TERM_DISPLAY)


class TmuxClaude(AgentSession):
    """The agent on camera: interactive Claude Code in tmux, in an xterm on
    its own display, recorded beside the app. The harness types each beat
    into it as a person would, and a Stop hook marks the end of each turn.
    The transcript is Claude Code's own session log, tailed as it grows."""

    def __init__(self, *args, procs, take_dir):
        super().__init__(*args)
        self.procs, self.take_dir = procs, take_dir
        self.allowed = self.ar.CLAUDE_BUILTIN_TOOLS + ["mcp__maleficium__" + t for t in self.tool_names]
        self.claude = shutil.which("claude") or "/usr/bin/claude"
        self.turns = os.path.join(self.home, ".agent-turns")
        self.settings = os.path.join(self.home, ".agent-settings.json")
        self.beat = None
        self.tail_halt = threading.Event()
        self.capture = None
        os.makedirs(os.path.join(self.agent_home, ".claude"), exist_ok=True, mode=0o700)
        self.write_mcp_config()
        with open(self.settings, "w") as f:
            json.dump({"hooks": {"Stop": [{"hooks": [{"type": "command",
                                                       "command": "date +%%s >> %s" % self.turns}]}]}}, f)

    def tmux(self, *args, env=None):
        return subprocess.run(["tmux", "-L", TMUX_SOCKET, *args], env=env, capture_output=True, text=True,
                              timeout=15)

    def start(self, project):
        with open(os.path.join(self.agent_home, ".claude.json"), "w") as f:
            json.dump({"hasCompletedOnboarding": True, "theme": "dark",
                       "projects": {project: {"hasTrustDialogAccepted": True,
                                              "hasCompletedProjectOnboarding": True}}}, f)
        argv = self.ar.sandbox(self.home, project, [
            self.claude, "--setting-sources", "", "--settings", self.settings,
            "--disable-slash-commands", "--restricted", "--tools", ",".join(self.ar.CLAUDE_BUILTIN_TOOLS),
            "--mcp-config", self.mcp_config, "--strict-mcp-config",
            "--allowedTools"] + self.allowed + ["--model", self.model])
        # The token reaches claude through the tmux server's environment: no
        # file, and nothing in a command line.
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "TERM": "xterm-256color", "HOME": self.agent_home,
               "LANG": "C.UTF-8", "CLAUDE_CODE_OAUTH_TOKEN": self.ar.claude_token(),
               # no "Update available!" notice on camera
               "DISABLE_AUTOUPDATER": "1"}
        self.tmux("kill-server")
        r = self.tmux("new-session", "-d", "-s", "agent", "-x", str(TERM_COLS), "-y", str(TERM_ROWS),
                      "-c", project, " ".join(shlex.quote(a) for a in argv), env=env)
        if r.returncode != 0:
            raise SystemExit("showreel: FATAL tmux did not start: %s" % r.stderr.strip())
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
            raise SystemExit("showreel: FATAL claude never showed its prompt in tmux")
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
        projects = os.path.join(self.agent_home, ".claude", "projects")
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
                    for line in chunk[:cut].decode("utf-8", "replace").splitlines():
                        try:
                            e = json.loads(line)
                        except ValueError:
                            continue
                        self.emit(e, self.beat)
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


def normalize_opencode(e):
    """An opencode --format json line as Claude-shaped message envelopes, so
    the director follows it unchanged: tool_use blocks name the edit or the
    maleficium tool, tool_result blocks carry its text and error flag."""
    out = []
    if e.get("type") != "tool_use":
        return out
    part = e.get("part", {}) or {}
    state = part.get("state", {}) or {}
    tool, inp = part.get("tool", ""), state.get("input") or {}
    text = state.get("output")
    text = text if isinstance(text, str) else json.dumps(text) if text is not None else ""
    if tool in ("edit", "write"):
        name = {"edit": "Edit", "write": "Write"}[tool]
        args = {"file_path": inp.get("path") or ""}
        if tool == "write" and not args["file_path"]:
            return out
        uid = "opencode-%s" % (part.get("id") or e.get("_ts_ms", ""))
        out.append({"message": {"content": [{"type": "tool_use", "id": uid, "name": name, "input": args}]}})
        if state.get("status") not in (None, "", "pending", "running"):
            out.append({"message": {"content": [{"type": "tool_result", "tool_use_id": uid,
                                                 "content": [{"text": text}],
                                                 "is_error": state.get("status") != "completed"}]}})
    elif tool == "execute":
        inner = ((state.get("metadata") or {}).get("metadata") or {}).get("toolCalls") or []
        for c in inner:
            name = c.get("tool", "") or ""
            if not name.startswith("maleficium."):
                continue
            uid = "opencode-%s" % (c.get("callId") or c.get("id") or e.get("_ts_ms", ""))
            out.append({"message": {"content": [{"type": "tool_use", "id": uid,
                                                 "name": "mcp__maleficium__" + name.split(".", 1)[1],
                                                 "input": c.get("input") or {}}]}})
            if (c.get("status") or "") not in ("", "pending", "running"):
                out.append({"message": {"content": [{"type": "tool_result", "tool_use_id": uid,
                                                     "content": [{"text": text}],
                                                     "is_error": c.get("status") != "completed"}]}})
    return out


class OpencodePrint(AgentSession):
    """Headless opencode, one session resumed for each beat (`opencode run
    --session`), streaming --format json. The app recording is the video;
    the transcript renders as the terminal pane in post."""

    def __init__(self, *args):
        super().__init__(*args)
        self.opencode = shutil.which("opencode") or "opencode"
        # opencode v2 session ids start with "ses".
        self.session = "ses" + uuid.uuid4().hex
        self.write_mcp_config()
        h = os.path.expanduser("~")
        self.extra_rw = [os.path.join(h, d) for d in (".local/share/opencode", ".cache/opencode",
                                                      ".local/state/opencode")]

    def write_mcp_config(self):
        with open(self.mcp_config, "w") as f:
            json.dump({"mcp": {"servers": {"maleficium": {
                "type": "local", "command": self.server.cmd,
                "environment": dict(self.server.env, HOME=self.home)}}}}, f, indent=2)

    def turn(self, beat, prompt, project, timeout):
        argv = ["env", "OPENCODE_CONFIG=" + self.mcp_config, self.opencode, "run", "--standalone", "--auto",
                "--print-logs", "--format", "json", "-m", self.model, "--session", self.session, prompt]
        env = dict(os.environ, HOME=self.agent_home)
        p = subprocess.Popen(self.ar.sandbox(self.home, project, argv, extra_rw=self.extra_rw),
                             stdout=subprocess.PIPE,
                             stderr=open(os.path.join(os.path.dirname(self.events_path), "opencode.stderr.log"), "a"),
                             stdin=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True, env=env)
        cost, errors = 0.0, False

        def pump():
            nonlocal cost, errors
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
                if e.get("type") == "step_finish":
                    cost += (e.get("part", {}) or {}).get("cost") or 0
                self.emit(e, beat)
                for env_e in normalize_opencode(e):
                    try:
                        self.on_event(env_e)
                    except Exception as ex:
                        print("showreel: director skipped an event: %s" % ex, flush=True)
                    for c in (env_e.get("message", {}) or {}).get("content", []):
                        errors |= c.get("type") == "tool_result" and bool(c.get("is_error"))

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
        self.cost += cost
        return {"exit": code, "timed_out": timed_out, "wall_s": round(time.time() - t0, 1), "cost": cost,
                "is_error": errors, "turns": None}
