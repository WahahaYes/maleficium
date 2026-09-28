#!/usr/bin/env python3
"""codrive-run.py — an agent edits and compiles over MCP while the real app is open.

"Agent writes, you watch the PDF", as a test. Per run:
  1. the scenario's fixture is generated (as agent-run.py does) and the real
     app is launched on it under Xvfb, with a scratch HOME that the agent's
     MCP server shares: the app and the server are separate processes, and
     the filesystem (the project, and the engine outputs under that HOME) is
     all they share;
  2. the window is placed at 0,0 at the full screen size (no window manager,
     no chrome), the project is opened and compiled once from the app (not
     with --fresh), and in contested mode the harness starts typing into the
     contested file, so its buffer holds unsaved edits when the agent writes
     that file;
  3. the agent runs headless over MCP through an agent-run.py runner, with
     the app open the whole time; in contested mode the user then compiles
     with the conflict open and reloads from disk;
  4. oracles from the app's own event log: the preview reloaded on the
     agent's compiles and shows the last one, every file the agent changed
     was seen as an external change, clean buffers took the agent's text,
     the app wrote nothing over the agent, and the contested buffer raised a
     conflict and held its writes instead of being reloaded over or saved
     over the agent's file. agent-run.py's artifact oracles then judge the
     project the agent left.
--mode race runs no agent: it writes the open file from outside around the
moment autosave fires and checks neither edit is lost.

Capture seam for screen recording: each run writes capture.json (display,
screen and window geometry, run start) and timeline.jsonl (epoch-ms marks on
the same clock as the app log and the agent's transcript).
CODRIVE_CAPTURE_CMD, if set, is started through sh once the window is placed
and the project is open, just before the agent starts, with DISPLAY,
CODRIVE_DISPLAY, CODRIVE_SCREEN (WxH) and CODRIVE_RUN_DIR in its environment;
it gets SIGINT when the run ends (e.g. `exec ffmpeg -f x11grab -video_size
$CODRIVE_SCREEN -i $DISPLAY $CODRIVE_RUN_DIR/screen.mp4`).

Usage:
  python3 e2e/codrive-run.py [-n N] [--mode contested|showcase|race] [--fresh]
                             [--runner opencode|claude|script] [--model M]
                             [--out DIR] [--budget USD] [--no-build]
Env: CODRIVE_DISPLAY (:97), CODRIVE_SCREEN (1920x1080), CODRIVE_PORT (1423,
the harness vite), CODRIVE_MIRROR_PORT (18791), CARGO_TARGET_DIR (default
/var/tmp/maleficium-codrive-target), CODRIVE_CAPTURE_CMD.
Needs: Xvfb, xdotool, ImageMagick import, bwrap, curl, and the runner's CLI.
Manual only: it spends model credits. See e2e/README.md.
"""
import argparse, importlib.util, inspect, json, os, shutil, signal, subprocess, sys, threading, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SCENARIO = os.path.join(HERE, "agent-scenarios", "codrive", "codrive.json")

_spec = importlib.util.spec_from_file_location("agent_run", os.path.join(HERE, "agent-run.py"))
ar = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ar)

DISP = os.environ.get("CODRIVE_DISPLAY", ":97")
SCREEN = os.environ.get("CODRIVE_SCREEN", "1920x1080")
PORT = int(os.environ.get("CODRIVE_PORT", "1423"))
MIRROR_PORT = int(os.environ.get("CODRIVE_MIRROR_PORT", "18791"))
TARGET = os.environ.get("CARGO_TARGET_DIR") or "/var/tmp/maleficium-codrive-target"
APPBIN = os.path.join(TARGET, "debug", "maleficium")
LOG_REL = os.path.join(".local", "share", ar.IDENT, "maleficium-log", "events.jsonl")
# Race mode: outside writes this long after the user's last key, around the
# 1.2 s autosave.
RACE_OFFSETS = [0.9, 1.0, 1.1, 1.2, 1.3, 1.4]
# The file tree's rows, top-left anchored (stills State 9): name-sorted.
TREE_X, TREE_Y0, TREE_DY = 70, 104, 26


def say(msg):
    print("codrive: " + msg, flush=True)


def die(msg):
    raise SystemExit("codrive: FATAL " + msg)


def now_ms():
    return int(time.time() * 1000)


def utf16_len(text):
    return len(text.encode("utf-16-le")) // 2


class Timeline:
    """Epoch-ms marks for the run, on the clock the app log and the agent's
    events use, so a recording and a transcript can be lined up with them."""

    def __init__(self, path):
        self.path = path
        self.marks = []
        open(path, "w").close()

    def mark(self, what, **fields):
        m = dict(at=now_ms(), what=what, **fields)
        self.marks.append(m)
        with open(self.path, "a") as f:
            f.write(json.dumps(m) + "\n")
        return m["at"]

    def at(self, what):
        return next((m["at"] for m in self.marks if m["what"] == what), None)


# ---- processes -------------------------------------------------------------------


class Procs:
    """Every long-lived process this harness starts, killed by group at exit."""

    def __init__(self):
        self.groups = []

    def start(self, argv, **kw):
        p = subprocess.Popen(argv, start_new_session=True, **kw)
        self.groups.append(p)
        return p

    def stop(self, p, sig=signal.SIGTERM, grace=5):
        if p.poll() is None:
            try:
                os.killpg(p.pid, sig)
            except ProcessLookupError:
                pass
            try:
                p.wait(timeout=grace)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(p.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                p.wait()
        # the leader may be gone while its group lives on (webkit children)
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
        if p in self.groups:
            self.groups.remove(p)

    def stop_all(self):
        for p in reversed(list(self.groups)):
            self.stop(p, grace=3)


def take_display_lock():
    """The stills harness's per-display lock, so the two never share one."""
    lock = "/tmp/maleficium-stills-display-%s.lock" % DISP.lstrip(":")
    try:
        os.mkdir(lock)
    except FileExistsError:
        try:
            holder = int(open(os.path.join(lock, "pid")).read().strip())
            os.kill(holder, 0)
            die("display %s is in use by pid %d (set CODRIVE_DISPLAY)" % (DISP, holder))
        except (ValueError, OSError):
            shutil.rmtree(lock, ignore_errors=True)
            os.mkdir(lock)
    with open(os.path.join(lock, "pid"), "w") as f:
        f.write(str(os.getpid()))
    return lock


def xenv():
    return dict(os.environ, DISPLAY=DISP)


def xdo(*args, timeout=15):
    try:
        r = subprocess.run(["xdotool", *args], env=xenv(), capture_output=True, text=True, timeout=timeout)
        return r.stdout
    except subprocess.TimeoutExpired:
        return ""


def start_xvfb(procs):
    if os.path.exists("/tmp/.X%s-lock" % DISP.lstrip(":")):
        die("an X server already holds %s (set CODRIVE_DISPLAY)" % DISP)
    w, h = SCREEN.split("x")
    p = procs.start(["Xvfb", DISP, "-screen", "0", "%sx%sx24" % (w, h), "-nolisten", "tcp"],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    end = time.time() + 20
    while time.time() < end:
        if subprocess.run(["xdotool", "getdisplaygeometry"], env=xenv(), capture_output=True).returncode == 0:
            return p
        if p.poll() is not None:
            die("Xvfb exited on %s" % DISP)
        time.sleep(0.3)
    die("Xvfb never answered on %s" % DISP)


def build_app(out):
    env = dict(os.environ, CARGO_TARGET_DIR=TARGET,
               PATH=os.path.expanduser("~/.cargo/bin") + os.pathsep + os.environ.get("PATH", ""),
               TAURI_CONFIG=json.dumps({"build": {"devUrl": "http://localhost:%d/" % PORT}}, separators=(",", ":")))
    t0 = time.time()
    with open(os.path.join(out, "build.log"), "w") as log:
        r = subprocess.run(["cargo", "build", "--bins"], cwd=os.path.join(ROOT, "src-tauri"), env=env,
                           stdout=log, stderr=subprocess.STDOUT, timeout=1800)
    if r.returncode != 0:
        die("app build failed (see %s/build.log)" % out)
    say("app built in %.0fs (%s)" % (time.time() - t0, APPBIN))


def start_vite(procs, out, preset):
    env = dict(os.environ, DEV_PORT=str(PORT), STILLS_PRESET_FILE=preset,
               VITE_CACHE_DIR=os.environ.get("VITE_CACHE_DIR", "/var/tmp/maleficium-codrive-vite-cache"))
    log = open(os.path.join(out, "vite.log"), "w")
    p = procs.start([os.path.join(ROOT, "node_modules", ".bin", "vite")], cwd=ROOT, env=env,
                    stdout=log, stderr=subprocess.STDOUT)
    end = time.time() + 60
    while time.time() < end:
        if subprocess.run(["curl", "-s", "-o", "/dev/null", "http://localhost:%d/" % PORT]).returncode == 0:
            return p
        if p.poll() is not None:
            die("vite exited (port %d held? see %s/vite.log)" % (PORT, out))
        time.sleep(0.5)
    die("vite never answered on :%d" % PORT)


def start_mirror(procs, out):
    err = open(os.path.join(out, "mirror.log"), "w")
    p = procs.start([sys.executable, os.path.join(HERE, "bundle-mirror.py"), str(MIRROR_PORT), ar.MIRROR_CACHE],
                    stdout=subprocess.PIPE, stderr=err, text=True)
    if "listening" not in (p.stdout.readline() or ""):
        die("bundle mirror did not start (port %d held? see %s/mirror.log)" % (MIRROR_PORT, out))
    return p


def make_server(bundle_url):
    """agent-run's Server for the harness's own build: the same binary the
    app runs, with --mcp, reading the bundle through this run's mirror."""
    s = ar.Server.__new__(ar.Server)
    s.kind = "codrive"
    s.cmd = [APPBIN, "--mcp"]
    s.env = {"MALEFICIUM_DEV_BUNDLE_URL": bundle_url}
    return s


# ---- the app ----------------------------------------------------------------------


class AppLog:
    def __init__(self, home):
        self.path = os.path.join(home, LOG_REL)

    def events(self):
        out = []
        try:
            with open(self.path) as f:
                for line in f:
                    try:
                        out.append(json.loads(line))
                    except ValueError:
                        pass
        except OSError:
            pass
        return out

    def find(self, action, since, pred=None):
        for e in self.events():
            ev = e.get("event") if isinstance(e.get("event"), dict) else {}
            if e.get("at", 0) > since and ev.get("action") == action and (pred is None or pred(ev)):
                return e
        return None

    def wait(self, action, since, timeout, pred=None):
        end = time.time() + timeout
        while time.time() < end:
            e = self.find(action, since, pred)
            if e:
                return e
            time.sleep(0.3)
        return None


class App:
    def __init__(self, procs, out, home, preset_file, bundle_url, tl):
        self.procs, self.out, self.home, self.tl = procs, out, home, tl
        self.preset_file, self.bundle_url = preset_file, bundle_url
        self.log = AppLog(home)
        self.p = None
        self.win = None

    def launch(self, project):
        with open(self.preset_file, "w") as f:
            f.write(project)
        env = {k: v for k, v in os.environ.items() if not k.startswith("XDG_")}
        env.update(HOME=self.home, RUSTUP_HOME=os.path.expanduser("~/.rustup"),
                   CARGO_HOME=os.path.expanduser("~/.cargo"), DISPLAY=DISP, GDK_BACKEND="x11",
                   WEBKIT_DISABLE_COMPOSITING_MODE="1", MALEFICIUM_DEV_BUNDLE_URL=self.bundle_url)
        env.pop("WAYLAND_DISPLAY", None)
        self.launched = self.tl.mark("app.launch", project=project)
        self.p = self.procs.start([APPBIN], cwd=os.path.join(ROOT, "src-tauri"), env=env,
                                  stdout=open(os.path.join(self.out, "app.log"), "w"), stderr=subprocess.STDOUT)

    def place(self, timeout=300):
        """Find the main window (the largest 'Maleficium'), map it, and pin it
        to 0,0 at the full screen size: with no window manager nothing else
        maximizes it, and a stable geometry is what a recording needs."""
        w, h = (int(x) for x in SCREEN.split("x"))
        end = time.time() + timeout
        while time.time() < end:
            if self.p.poll() is not None:
                die("the app exited (see %s/app.log)" % self.out)
            best, win = 0, None
            for cand in xdo("search", "--name", "^Maleficium$").split():
                geo = xdo("getwindowgeometry", cand)
                size = [ln.split()[-1] for ln in geo.splitlines() if "Geometry:" in ln]
                if size:
                    cw, ch = (int(x) for x in size[0].split("x"))
                    if cw * ch > best:
                        best, win = cw * ch, cand
            if win:
                xdo("windowmap", win)
                xdo("windowmove", win, "0", "0")
                xdo("windowsize", win, str(w), str(h))
                vend = time.time() + 30
                while time.time() < vend:
                    geo = xdo("getwindowgeometry", win)
                    if win in xdo("search", "--onlyvisible", "--name", "^Maleficium$").split() \
                            and "Geometry: %dx%d" % (w, h) in geo:
                        self.win = win
                        pos = [ln.split()[1] for ln in geo.splitlines() if "Position:" in ln]
                        self.tl.mark("app.window", window=win, geometry="%dx%d+%s" % (w, h, (pos or ["?"])[0]))
                        return win
                    xdo("windowsize", win, str(w), str(h))
                    time.sleep(0.5)
                die("window %s never mapped at %dx%d: %s" % (win, w, h, geo.strip()))
            time.sleep(1)
        die("no app window appeared (see %s/app.log)" % self.out)

    def focus(self):
        xdo("windowraise", self.win)
        xdo("windowfocus", "--sync", self.win)

    def key(self, *keys, to_window=True):
        """Menu accelerators need --window; webview keys go to the focus."""
        self.focus()
        if to_window:
            xdo("key", "--window", self.win, *keys)
        else:
            xdo("key", *keys)
        time.sleep(0.5)

    def click(self, x, y, repeat=1):
        xdo("mousemove", "--window", self.win, str(x), str(y), "click", "--repeat", str(repeat), "1")
        time.sleep(0.8)

    def shot(self, name):
        path = os.path.join(self.out, name + ".png")
        r = subprocess.run(["import", "-display", DISP, "-window", self.win, path], capture_output=True, timeout=60)
        if r.returncode != 0:
            die("capture failed: %s" % name)
        self.tl.mark("shot", file=path)
        return path

    def open_project(self):
        if not self.log.wait("log.open", self.launched - 1, 90):
            die("the app never opened its event log (%s)" % self.log.path)
        m = now_ms()
        for attempt in range(3):
            self.key("ctrl+o")
            e = self.log.wait("file.open", m, 12)
            if e:
                self.tl.mark("project.open", file=e["event"]["path"])
                return e["event"]["path"]
            say("no file.open after Ctrl+O (try %d)" % (attempt + 1))
        die("Ctrl+O never opened the preset project")

    def open_tree_row(self, project, rows, name):
        m = now_ms()
        self.click(TREE_X, TREE_Y0 + TREE_DY * rows.index(name))
        want = os.path.join(project, name)
        e = self.log.wait("file.open", m, 15, lambda ev: ev.get("path") == want) or \
            self.log.wait("file.switch", m, 1, lambda ev: ev.get("path") == want)
        if not e:
            die("clicking the tree row for %s did not open it" % name)
        return want

    def stop(self):
        if self.p:
            self.procs.stop(self.p, grace=5)
            self.tl.mark("app.stop")
            self.p = None


class Typist(threading.Thread):
    """The user, typing into the active buffer: a key every 0.5 s keeps it
    dirty (autosave waits for 1.2 s of quiet), until the app reports a
    conflict on that file or the run says stop."""

    def __init__(self, app, path, marker, tl):
        super().__init__(daemon=True)
        self.app, self.path, self.marker, self.tl = app, path, marker, tl
        self.halt = threading.Event()
        self.keys = 0

    def run(self):
        xdo("type", "--delay", "30", self.marker)
        self.tl.mark("typing.start", path=self.path)
        started = now_ms()
        while not self.halt.is_set():
            if self.app.log.find("file.external-conflict", started, lambda ev: ev.get("path") == self.path):
                self.tl.mark("typing.stop", reason="conflict", keys=self.keys)
                return
            xdo("type", "--delay", "0", ".")
            self.keys += 1
            self.halt.wait(0.5)
        self.tl.mark("typing.stop", reason="halted", keys=self.keys)


# ---- the agent --------------------------------------------------------------------


def agent_home(runner, agent_dir):
    """The MCP server's HOME for a runner: the app is launched on the same one."""
    return os.path.join(agent_dir, "mcp-home" if runner == "claude" else "home")


def run_agent(runner, scenario, server, model, agent_dir, project, home, tl):
    """agent-run.py's runner, on a project and home that already exist (the
    app has them open): its fixture and cache seeding are skipped for the
    call, everything else (MCP config, sandbox, timeout, transcript) is its."""
    fn = getattr(ar, "run_agent_" + runner, None) or (getattr(ar, "run_agent", None) if runner == "opencode" else None)
    read = getattr(ar, "read_events_" + runner, None) or \
        (getattr(ar, "read_events", None) if runner == "opencode" else None)
    if not fn or not read:
        die("agent-run.py has no %s runner" % runner)
    args = [scenario, server, model, agent_dir, None]
    if runner == "claude":
        defaults = inspect.signature(ar.run_scenario_once).parameters
        args += [ar.discover_tool_names(server), defaults["claude_max_turns"].default,
                 defaults["claude_budget_usd"].default, shutil.which("claude")]
    saved = ar.make_fixture, ar.seed_home
    ar.make_fixture = ar.seed_home = lambda *a, **k: None
    tl.mark("agent.start", runner=runner, model=model)
    try:
        r = fn(*args)
    finally:
        ar.make_fixture, ar.seed_home = saved
    tl.mark("agent.exit", code=r["exit"], timed_out=r["timed_out"])
    metrics, calls, other, texts = read(os.path.join(agent_dir, "events.jsonl"))
    return dict(r, metrics=metrics, calls=calls, other=other, final_text=texts[-1] if texts else "")


def run_script(runner, scenario, server, model, agent_dir, project, home, tl):
    """No model: the scenario's solution, one edit at a time with an MCP
    compile after each, paced like an agent. Checks the harness and the app
    oracles for free (the stand-in for agent-run.py's --self-test)."""
    before = ar.snapshot(agent_dir, skip=("home",))
    open(os.path.join(agent_dir, "events.jsonl"), "w").close()
    t0 = time.time()
    tl.mark("agent.start", runner="script", model=None)
    mcp = ar.Mcp(server, home)
    calls = []
    try:
        ok, err = mcp.call("grant", {"root_id": "script", "root": project})
        calls.append({"tool": "grant", "input": {"root": project}, "ok": ok, "status": None, "error": None})
        for edit in scenario["solution"]:
            time.sleep(4)
            ar.apply_edits(project, [edit])
            tl.mark("script.edit", file=edit["file"])
            time.sleep(2)
            rec = mcp.compile("script", scenario["codrive"]["main"])
            calls.append({"tool": "compile_run", "input": {"rel": scenario["codrive"]["main"]},
                          "ok": rec.get("status") == "success", "status": rec.get("status"), "error": None})
            tl.mark("script.compiled", status=rec.get("status"))
    finally:
        mcp.close()
    tl.mark("agent.exit", code=0, timed_out=False)
    metrics = {"steps": 0, "tokens_in": 0, "tokens_out": 0, "tokens_reasoning": 0, "cost": 0.0,
               "mcp_calls": len(calls), "mcp_errors": sum(1 for c in calls if not c["ok"]), "other_calls": 0}
    return {"exit": 0, "timed_out": False, "wall_s": round(time.time() - t0, 1), "before": before,
            "prompt": None, "metrics": metrics, "calls": calls, "other": [], "final_text": ""}


RUNNERS = ["opencode", "claude", "script"]


# ---- oracles from the app log -----------------------------------------------------


def judge_app(events, tl, project, fixture_text, final_text, mode, contested, marker, stamp):
    """Each oracle is (name, ok, detail). events: the app's log, parsed."""
    t_agent, t_exit = tl.at("agent.start"), tl.at("agent.exit")
    t_type = tl.at("typing.start")
    t_resolve = tl.at("resolve") or float("inf")
    evs = [(e.get("at", 0), e.get("actor"), e.get("event") if isinstance(e.get("event"), dict) else {}) for e in events]

    def acts(name, a=0, b=float("inf"), path=None):
        return [(at, actor, ev) for at, actor, ev in evs if ev.get("action") == name and a < at <= b
                and (path is None or ev.get("path") == path)]

    out = []
    # log sanity: this launch's log, one object per line, every line an actor
    opens = len(acts("log.open"))
    no_actor = sum(1 for e in events if e.get("actor") not in ("user", "agent", "system"))
    out.append(("app_log_sane", opens == 1 and no_actor == 0 and bool(events),
                "%d lines, %d log.open, %d without an actor" % (len(events), opens, no_actor)))

    # the preview followed the agent's compiles and shows the last one
    ext = acts("preview.external-update", t_agent)
    renders = acts("preview.page-render", t_agent)
    failed = acts("preview.load-failed", t_agent) + acts("preview.stamp-failed", t_agent)
    if stamp is None:
        out.append(("preview_follows_agent", False, "no pdf after the run"))
    else:
        last = [x for x in ext if x[0] >= stamp["mtimeMs"]]
        painted = last and any(r[0] > last[0][0] for r in renders)
        ok = bool(ext) and bool(last) and bool(painted) and not failed
        out.append(("preview_follows_agent", ok,
                    "%d external reloads after the agent started; last pdf written %s, reloaded %s, repainted %s%s" % (
                        len(ext), stamp["mtimeMs"], last[0][0] if last else "never",
                        "yes" if painted else "no", "; failures: %s" % failed[:2] if failed else "")))

    # every file the agent changed was seen changing on disk
    changed = sorted(rel for rel in final_text if final_text[rel] != fixture_text.get(rel))
    unseen = [rel for rel in changed if not acts("fs.external", t_agent, path=os.path.join(project, rel))]
    out.append(("external_edits_detected", bool(changed) and not unseen,
                "changed by the agent: %s; not seen: %s" % (changed, unseen or "none")))

    # clean open buffers took the agent's text (the last reload is the final file)
    opened = {ev.get("path") for _, _, ev in evs if ev.get("action") in ("file.open", "file.switch")}
    clean = [rel for rel in changed if os.path.join(project, rel) in opened and rel != contested]
    bad = []
    for rel in clean:
        rel_reloads = acts("file.reload", t_agent, path=os.path.join(project, rel))
        want = utf16_len(final_text[rel])
        if not rel_reloads or rel_reloads[-1][2].get("chars") != want or rel_reloads[-1][1] != "system":
            bad.append("%s (reloads %s, disk %d chars)" % (rel, [r[2].get("chars") for r in rel_reloads], want))
    out.append(("clean_buffers_follow_disk", bool(clean) and not bad,
                "%s took the disk text%s" % (clean, "; wrong: %s" % bad if bad else "")))

    # the app wrote nothing into the project while the agent worked
    saves = [ev.get("path") for _, _, ev in evs if ev.get("action") == "file.save"]
    late = [p for (at, _, ev) in acts("file.save", t_agent, t_resolve) for p in [ev.get("path")]]
    fails = [ev for (_, _, ev) in acts("file.save-failed", t_agent, t_resolve)
             if not (ev.get("path") == os.path.join(project, contested or "") and ev.get("trigger") == "auto"
                     and "changed on disk" in ev.get("error", ""))]
    out.append(("no_app_writes_over_agent", not late and not fails,
                "saves during the run: %s; unexpected save failures: %s (saves in the whole log: %d)" % (
                    late or "none", fails[:2] or "none", len(saves))))

    if mode == "contested":
        cpath = os.path.join(project, contested)
        conflicts = acts("file.external-conflict", t_agent, path=cpath)
        reloaded = acts("file.reload", t_type or 0, t_resolve, path=cpath)
        saved = acts("file.save", t_type or 0, t_resolve, path=cpath)
        held = [ev for (_, _, ev) in acts("file.save-failed", t_agent, t_resolve, path=cpath)
                if ev.get("trigger") == "auto" and "changed on disk" in ev.get("error", "")]
        on_disk = final_text.get(contested, "")
        ok = bool(t_type) and t_type < t_agent and bool(conflicts) and not reloaded and not saved \
            and bool(held) and marker.strip() not in on_disk
        out.append(("contested_buffer_not_clobbered", ok,
                    "typing from %s; %d conflicts; %d reloads and %d saves of the dirty buffer; %d autosaves held; "
                    "marker on disk: %s" % (t_type, len(conflicts), len(reloaded), len(saved), len(held),
                                            marker.strip() in on_disk)))
        res = acts("file.reload", t_resolve, path=cpath)
        want = utf16_len(on_disk)
        ok = bool(res) and res[0][1] == "user" and res[0][2].get("chars") == want
        out.append(("conflict_resolves_to_disk", ok,
                    "user reload %s (disk %d chars)" % ([(r[1], r[2].get("chars")) for r in res[:2]], want)))
    return out


# ---- one run ----------------------------------------------------------------------


def read_texts(project, names):
    out = {}
    for rel in names:
        t = ar.read_file(project, rel)
        if t is not None:
            out[rel] = t
    return out


def pdf_stamp(server, home, project, main):
    mcp = ar.Mcp(server, home)
    try:
        ok, _ = mcp.call("grant", {"root_id": "stamp", "root": project})
        if not ok:
            return None
        ok, r = mcp.call("output_stamp", {"root_id": "stamp", "main_rel": main})
        return (r or {}).get("stamp") if ok else None
    finally:
        mcp.close()


def one_run(s, server, warm, runner, model, mode, run_base, procs, preset_file, bundle_url, fresh=False):
    cd = s["codrive"]
    agent_dir = os.path.join(run_base, "agent")
    project = os.path.join(agent_dir, "project")
    home = agent_home(runner, agent_dir)
    os.makedirs(agent_dir)
    tl = Timeline(os.path.join(run_base, "timeline.jsonl"))
    tl.mark("run.start", scenario=s["id"], mode=mode, runner=runner, model=model)
    ar.make_fixture(s, project)
    ar.seed_home(warm, home)
    for rel, text in s.get("outside", {}).items():
        with open(os.path.join(agent_dir, rel), "w") as f:
            f.write(text)
    names = sorted(os.listdir(project))
    fixture_text = read_texts(project, names)
    fixture = ar.snapshot(project)

    app = App(procs, run_base, home, preset_file, bundle_url, tl)
    typist = None
    capture = None
    try:
        app.launch(project)
        win = app.place()
        app.open_project()
        w, h = SCREEN.split("x")
        with open(os.path.join(run_base, "capture.json"), "w") as f:
            json.dump({"display": DISP, "screen": SCREEN, "window": win, "geometry": {"x": 0, "y": 0, "w": int(w),
                       "h": int(h)}, "run_start_ms": tl.at("run.start"), "timeline": "timeline.jsonl",
                       "app_log": os.path.relpath(app.log.path, run_base),
                       "agent_events": "agent/events.jsonl"}, f, indent=2)
        # The first preview: the app builds the fixture once, as a user would
        # with Ctrl+R, so the agent's compiles are rewrites of a shown pdf.
        if fresh:
            # Never compiled: the app skips its warm build and shows no pdf.
            if not app.log.wait("compile.warm-skipped", app.launched - 1, 30):
                die("a fresh project did not skip the warm build (see %s)" % app.log.path)
            time.sleep(2)
        else:
            m = now_ms()
            app.click(450, 200)
            app.key("ctrl+r")
            if not app.log.wait("preview.page-render", m, 300):
                die("the fixture's first compile never painted (see %s)" % app.log.path)
            tl.mark("preview.first")
        if mode == "contested":
            app.open_tree_row(project, cd["tree_rows"], cd["contested"])
            app.click(450, 200)
            app.key("ctrl+End", to_window=False)
            typist = Typist(app, os.path.join(project, cd["contested"]), cd["marker"], tl)
            typist.start()
            time.sleep(3)
        app.shot("01-before")
        if os.environ.get("CODRIVE_CAPTURE_CMD"):
            env = dict(os.environ, DISPLAY=DISP, CODRIVE_DISPLAY=DISP, CODRIVE_SCREEN=SCREEN, CODRIVE_RUN_DIR=run_base)
            capture = procs.start(["sh", "-c", os.environ["CODRIVE_CAPTURE_CMD"]], env=env,
                                  stdout=open(os.path.join(run_base, "capture.log"), "w"), stderr=subprocess.STDOUT)
            tl.mark("capture.start")
        r = (run_script if runner == "script" else run_agent)(runner, s, server, model, agent_dir, project, home, tl)
        if typist:
            typist.halt.set()
            typist.join(10)
        # Let the preview catch up with the last compile (it polls every 1.5 s).
        stamp = pdf_stamp(server, home, project, cd["main"])
        if stamp:
            app.log.wait("preview.external-update", stamp["mtimeMs"] - 1, 20)
            app.log.wait("preview.page-render", stamp["mtimeMs"] - 1, 20)
        time.sleep(2)
        tl.mark("settled", pdf=stamp)
        final_text = read_texts(project, names)
        if mode == "contested":
            app.shot("02-conflict")
            if app.log.find("file.external-conflict", tl.at("agent.start"),
                            lambda ev: ev.get("path") == os.path.join(project, cd["contested"])):
                # The user compiles with the conflict still open: the compile
                # persists dirty buffers, and must leave the held one alone.
                m = tl.mark("user.compile")
                app.key("ctrl+r")
                app.log.wait("compile.finish", m, 180)
                time.sleep(2)
                # The dialog's buttons: Keep my edits, then Reload from disk.
                tl.mark("resolve")
                app.focus()
                xdo("key", "Tab", "Tab", "Return")
                app.log.wait("file.reload", tl.at("resolve"), 10)
                time.sleep(1)
        app.shot("03-after")
        events = app.log.events()
        shutil.copy(app.log.path, os.path.join(run_base, "app-events.jsonl"))
    finally:
        if typist:
            typist.halt.set()
        if capture:
            procs.stop(capture, sig=signal.SIGINT, grace=15)
            tl.mark("capture.stop")
        app.stop()
    app_oracles = judge_app(events, tl, project, fixture_text, final_text, mode,
                            cd["contested"] if mode == "contested" else None, cd["marker"], stamp)
    own = {"own": ("claude-home", "mcp-home", "claude.stderr.log")} if runner == "claude" else {}
    judged = ar.Judge(s, server, project, home, fixture, r["calls"], agent_dir, r["before"], **own).run()
    judged = [{"oracle": n, "ok": bool(ok), "detail": d, "from": "app-log"} for n, ok, d in app_oracles] + \
             [dict(j, **{"from": "artifacts"}) for j in judged]
    passed = not r["timed_out"] and all(j["ok"] for j in judged)
    tl.mark("run.end", passed=passed)
    return {"scenario": s["id"], "mode": mode, "runner": runner, "model": model, "passed": passed,
            "timed_out": r["timed_out"], "exit": r["exit"], "wall_s": r["wall_s"], "metrics": r["metrics"],
            "oracles": judged, "calls": r["calls"], "other_tools": r["other"], "final_text": r["final_text"],
            "timeline": tl.marks, "typed_keys": typist.keys if typist else 0}


def race_trial(s, warm, offset, base, procs, preset_file, bundle_url):
    """One outside write `offset` s after the user's last key, around the
    moment autosave fires (1.2 s of quiet): neither side may be lost. The
    write reads the file and writes it back, as an agent's edit tool does."""
    project, home = os.path.join(base, "project"), os.path.join(base, "home")
    os.makedirs(base)
    ar.make_fixture(s, project)
    ar.seed_home(warm, home)
    tl = Timeline(os.path.join(base, "timeline.jsonl"))
    app = App(procs, base, home, preset_file, bundle_url, tl)
    main = os.path.join(project, s["codrive"]["main"])
    try:
        app.launch(project)
        app.place()
        app.open_project()
        app.click(450, 200)
        app.key("ctrl+End", to_window=False)
        m = now_ms()
        xdo("type", "--delay", "20", "% typed earlier")
        if not app.log.wait("file.save", m, 20, lambda ev: ev.get("path") == main):
            die("the typed burst was never autosaved (%s)" % app.log.path)
        time.sleep(1)
        xdo("type", "--delay", "0", "Q")
        t_key = tl.mark("user.key")
        time.sleep(offset)
        with open(main) as f:
            text = f.read()
        with open(main, "w") as f:
            f.write(text.replace("\\end{document}", "% written outside\n\\end{document}"))
        t_write = tl.mark("outside.write")
        time.sleep(5)
        with open(main) as f:
            disk = f.read()
        events = app.log.events()
        shutil.copy(app.log.path, os.path.join(base, "app-events.jsonl"))
    finally:
        app.stop()

    def acts(name, since):
        return [e for e in events if (e.get("event") or {}).get("action") == name and e.get("at", 0) > since
                and (e.get("event") or {}).get("path") == main]
    conflict = acts("file.external-conflict", t_write)
    reload = [e for e in acts("file.reload", t_write) if e["event"].get("chars") == utf16_len(disk)]
    refused = [e for e in acts("file.save-failed", t_key) if "changed on disk" in e["event"].get("error", "")]
    kept_outside = "% written outside" in disk
    # the key is on disk, or waits in the dirty buffer behind a conflict
    kept_key = ("Q" in disk.replace("% written outside", "")) or bool(conflict)
    ok = kept_outside and kept_key and bool(conflict or reload)
    return {"offset": offset, "passed": ok, "outside_kept": kept_outside, "key_kept": kept_key,
            "conflict": bool(conflict), "reloaded": bool(reload), "autosave_refused": bool(refused),
            "write_after_key_ms": t_write - t_key}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--runner", default="opencode", choices=RUNNERS,
                    help="an agent-run.py runner, or script (the solution, no model)")
    ap.add_argument("--model", help="default: agent-run.py's for the runner")
    ap.add_argument("--mode", default="contested", choices=["contested", "showcase", "race"],
                    help="contested: the user types in the contested file while the agent works; "
                         "showcase: hands off, for recordings; race: no agent, outside writes timed "
                         "against autosave")
    ap.add_argument("--fresh", action="store_true",
                    help="open the project never compiled, so the agent's first compile is the first pdf")
    ap.add_argument("-n", type=int, default=1)
    ap.add_argument("--out", help="results dir (default /var/tmp/maleficium-codrive-runs/<stamp>)")
    ap.add_argument("--budget", type=float, default=5.0)
    ap.add_argument("--no-build", action="store_true", help="use the existing build in CARGO_TARGET_DIR")
    a = ap.parse_args()
    agent = a.mode != "race" and a.runner != "script"
    a.model = a.model or (getattr(ar, "CLAUDE_MODEL", None) if a.runner == "claude" else ar.FREE_MODEL)
    for tool in ["Xvfb", "xdotool", "import", "bwrap", "curl"] + ([a.runner] if agent else []):
        if not shutil.which(tool):
            die("needs %s on PATH" % tool)
    if agent and a.runner == "claude":
        ar.claude_token()
    with open(SCENARIO) as f:
        s = json.load(f)
    out = a.out or os.path.join("/var/tmp/maleficium-codrive-runs", time.strftime("%Y%m%d-%H%M%S"))
    os.makedirs(out, exist_ok=True)
    procs = Procs()
    lock = take_display_lock()

    def on_signal(sig, _):
        raise SystemExit("codrive: stopped by signal %d" % sig)
    signal.signal(signal.SIGTERM, on_signal)
    results, spent = [], 0.0
    try:
        if not a.no_build:
            build_app(out)
        if not os.access(APPBIN, os.X_OK):
            die("no app build at %s" % APPBIN)
        mirror = start_mirror(procs, out)
        bundle_url = "http://127.0.0.1:%d/tlextras-2022.0r0.tar" % MIRROR_PORT
        server = make_server(bundle_url)
        warm = ar.warm_cache(server, s)
        preset = os.path.join(out, ".preset")
        open(preset, "w").close()
        start_vite(procs, out, preset)
        start_xvfb(procs)
        say("display %s (%s), vite :%d, mirror :%d, out %s" % (DISP, SCREEN, PORT, MIRROR_PORT, out))
        if a.mode == "race":
            trials = [race_trial(s, warm, off, os.path.join(out, "race-%.2f" % off), procs, preset, bundle_url)
                      for off in RACE_OFFSETS]
            with open(os.path.join(out, "race.json"), "w") as f:
                json.dump(trials, f, indent=2)
            for r in trials:
                say("  %s write %d ms after the key: outside edit kept %s, key kept %s, %s%s" % (
                    "PASS" if r["passed"] else "FAIL", r["write_after_key_ms"], r["outside_kept"], r["key_kept"],
                    "conflict" if r["conflict"] else "reloaded" if r["reloaded"] else "no report",
                    ", autosave refused" if r["autosave_refused"] else ""))
            sys.exit(0 if all(r["passed"] for r in trials) else 1)
        for i in range(a.n):
            if spent >= a.budget:
                say("budget $%.2f reached; stopping" % a.budget)
                break
            run_base = os.path.join(out, "%s-%s-%d" % (s["id"], a.mode, i + 1))
            say("run %s (%s, %s) ..." % (os.path.basename(run_base), a.runner, a.model))
            res = one_run(s, server, warm, a.runner, a.model, a.mode, run_base, procs, preset, bundle_url, a.fresh)
            spent += res["metrics"]["cost"]
            with open(os.path.join(run_base, "result.json"), "w") as f:
                json.dump(res, f, indent=2)
            results.append(res)
            say("  %s in %.0fs, %d MCP calls (%d errors), %d keys typed, $%.4f (total $%.4f)%s" % (
                "PASS" if res["passed"] else "FAIL", res["wall_s"], res["metrics"]["mcp_calls"],
                res["metrics"]["mcp_errors"], res["typed_keys"], res["metrics"]["cost"], spent,
                " TIMEOUT" if res["timed_out"] else ""))
            for j in res["oracles"]:
                say("    %s %s: %s" % ("ok " if j["ok"] else "BAD", j["oracle"], j["detail"]))
        lines = ["| run | mode | pass | wall s | MCP calls | preview reloads | cost $ |", "| --- | --- | --- | --- | --- | --- | --- |"]
        for i, r in enumerate(results):
            reloads = next((j["detail"].split(" ")[0] for j in r["oracles"] if j["oracle"] == "preview_follows_agent"), "?")
            lines.append("| %d | %s | %s | %s | %d | %s | %s |" % (i + 1, r["mode"], "PASS" if r["passed"] else "FAIL",
                                                           r["wall_s"], r["metrics"]["mcp_calls"], reloads,
                                                           r["metrics"]["cost"]))
        with open(os.path.join(out, "summary.md"), "w") as f:
            f.write("\n".join(lines) + "\n")
        print("\n".join(lines))
        say("results in %s; %d/%d passed; total reported cost $%.4f" % (
            out, sum(r["passed"] for r in results), len(results), spent))
    finally:
        procs.stop_all()
        shutil.rmtree(lock, ignore_errors=True)
    sys.exit(0 if results and all(r["passed"] for r in results) else 1)


if __name__ == "__main__":
    main()
