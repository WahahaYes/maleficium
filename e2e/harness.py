#!/usr/bin/env python3
"""harness.py — shared setup for the harnesses that drive the real app.

codrive-run.py and showreel-run.py both launch the app built from this
checkout under Xvfb, serve its frontend from a vite dev server, and feed
its engine bundle through a local mirror, so takes never depend on the
network past the first cache fill. Everything here is parameterized by
the caller (display, screen, ports, target dir); the runners keep their
own env names and defaults, so sharing this changes no behavior.
"""
import json
import os
import shutil
import signal
import subprocess
import sys
import time


# The file tree's rows, top-left anchored: name-sorted.
TREE_X, TREE_Y0, TREE_DY = 70, 104, 26


def now_ms():
    return int(time.time() * 1000)


class Timeline:
    """Epoch-ms marks for a run, on the clock the app log and the agent's
    transcript use, so a recording and a transcript can be lined up."""

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


class Procs:
    """Every long-lived process a harness starts, killed by group at exit."""

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


def take_display_lock(display):
    """The per-display lock, so two GUI harnesses never share one."""
    lock = "/tmp/maleficium-stills-display-%s.lock" % display.lstrip(":")
    try:
        os.mkdir(lock)
    except FileExistsError:
        try:
            holder = int(open(os.path.join(lock, "pid")).read().strip())
            os.kill(holder, 0)
            raise SystemExit("display %s is in use by pid %d" % (display, holder))
        except (ValueError, OSError):
            shutil.rmtree(lock, ignore_errors=True)
            os.mkdir(lock)
    with open(os.path.join(lock, "pid"), "w") as f:
        f.write(str(os.getpid()))
    return lock


def xdo(display, *args, timeout=15):
    try:
        r = subprocess.run(["xdotool", *args], env=dict(os.environ, DISPLAY=display),
                           capture_output=True, text=True, timeout=timeout)
        return r.stdout
    except subprocess.TimeoutExpired:
        return ""


def start_xvfb(procs, display, screen):
    if os.path.exists("/tmp/.X%s-lock" % display.lstrip(":")):
        raise SystemExit("an X server already holds %s" % display)
    w, h = screen.split("x")
    p = procs.start(["Xvfb", display, "-screen", "0", "%sx%sx24" % (w, h), "-nolisten", "tcp"],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    end = time.time() + 20
    while time.time() < end:
        if subprocess.run(["xdotool", "getdisplaygeometry"], env=dict(os.environ, DISPLAY=display),
                          capture_output=True).returncode == 0:
            return p
        if p.poll() is not None:
            raise SystemExit("Xvfb exited on %s" % display)
        time.sleep(0.3)
    raise SystemExit("Xvfb never answered on %s" % display)


def build_app(out, root, target, port, say=print):
    env = dict(os.environ, CARGO_TARGET_DIR=target,
               PATH=os.path.expanduser("~/.cargo/bin") + os.pathsep + os.environ.get("PATH", ""),
               TAURI_CONFIG=json.dumps({"build": {"devUrl": "http://localhost:%d/" % port}}, separators=(",", ":")))
    t0 = time.time()
    with open(os.path.join(out, "build.log"), "w") as log:
        r = subprocess.run(["cargo", "build", "--bins"], cwd=os.path.join(root, "src-tauri"), env=env,
                           stdout=log, stderr=subprocess.STDOUT, timeout=1800)
    if r.returncode != 0:
        raise SystemExit("app build failed (see %s/build.log)" % out)
    say("app built in %.0fs" % (time.time() - t0))


def start_vite(procs, out, root, preset, port, cache, say=print):
    env = dict(os.environ, DEV_PORT=str(port), STILLS_PRESET_FILE=preset, VITE_CACHE_DIR=cache)
    log = open(os.path.join(out, "vite.log"), "w")
    p = procs.start([os.path.join(root, "node_modules", ".bin", "vite")], cwd=root, env=env,
                    stdout=log, stderr=subprocess.STDOUT)
    end = time.time() + 60
    while time.time() < end:
        if subprocess.run(["curl", "-s", "-o", "/dev/null", "http://localhost:%d/" % port]).returncode == 0:
            return p
        if p.poll() is not None:
            raise SystemExit("vite exited (port %d held? see %s/vite.log)" % (port, out))
        time.sleep(0.5)
    raise SystemExit("vite never answered on :%d" % port)


def start_mirror(procs, out, here, mirror_port, mirror_cache):
    err = open(os.path.join(out, "mirror.log"), "w")
    p = procs.start([sys.executable, os.path.join(here, "bundle-mirror.py"), str(mirror_port), mirror_cache],
                    stdout=subprocess.PIPE, stderr=err, text=True)
    if "listening" not in (p.stdout.readline() or ""):
        raise SystemExit("bundle mirror did not start (port %d held? see %s/mirror.log)" % (mirror_port, out))
    return p


def make_server(appbin, bundle_url, kind, agent_run):
    """agent-run's Server for the harness's own build: the same binary the
    app runs, with --mcp, reading the bundle through this run's mirror."""
    s = agent_run.Server.__new__(agent_run.Server)
    s.kind = kind
    s.cmd = [appbin, "--mcp"]
    s.env = {"MALEFICIUM_DEV_BUNDLE_URL": bundle_url}
    return s


class AppLog:
    def __init__(self, home, ident):
        self.path = os.path.join(home, ".local", "share", ident, "maleficium-log", "events.jsonl")

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
    def __init__(self, procs, out, home, preset_file, bundle_url, tl, appbin, root, display, screen, ident):
        self.procs, self.out, self.home, self.tl = procs, out, home, tl
        self.preset_file, self.bundle_url = preset_file, bundle_url
        self.appbin, self.root, self.display, self.screen = appbin, root, display, screen
        self.log = AppLog(home, ident)
        self.p = None
        self.win = None

    def launch(self, project):
        with open(self.preset_file, "w") as f:
            f.write(project)
        env = {k: v for k, v in os.environ.items() if not k.startswith("XDG_")}
        env.update(HOME=self.home, RUSTUP_HOME=os.path.expanduser("~/.rustup"),
                   CARGO_HOME=os.path.expanduser("~/.cargo"), DISPLAY=self.display, GDK_BACKEND="x11",
                   WEBKIT_DISABLE_COMPOSITING_MODE="1", MALEFICIUM_DEV_BUNDLE_URL=self.bundle_url)
        env.pop("WAYLAND_DISPLAY", None)
        self.launched = self.tl.mark("app.launch", project=project)
        self.p = self.procs.start([self.appbin], cwd=os.path.join(self.root, "src-tauri"), env=env,
                                  stdout=open(os.path.join(self.out, "app.log"), "w"), stderr=subprocess.STDOUT)

    def place(self, timeout=300):
        """Find the main window (the largest 'Maleficium'), map it, and pin it
        to 0,0 at the full screen size: with no window manager nothing else
        maximizes it, and a stable geometry is what a recording needs."""
        w, h = (int(x) for x in self.screen.split("x"))
        end = time.time() + timeout
        while time.time() < end:
            if self.p.poll() is not None:
                raise SystemExit("the app exited (see %s/app.log)" % self.out)
            best, win = 0, None
            for cand in xdo(self.display, "search", "--name", "^Maleficium$").split():
                geo = xdo(self.display, "getwindowgeometry", cand)
                size = [ln.split()[-1] for ln in geo.splitlines() if "Geometry:" in ln]
                if size:
                    cw, ch = (int(x) for x in size[0].split("x"))
                    if cw * ch > best:
                        best, win = cw * ch, cand
            if win:
                xdo(self.display, "windowmap", win)
                xdo(self.display, "windowmove", win, "0", "0")
                xdo(self.display, "windowsize", win, str(w), str(h))
                vend = time.time() + 30
                while time.time() < vend:
                    geo = xdo(self.display, "getwindowgeometry", win)
                    if win in xdo(self.display, "search", "--onlyvisible", "--name", "^Maleficium$").split() \
                            and "Geometry: %dx%d" % (w, h) in geo:
                        self.win = win
                        pos = [ln.split()[1] for ln in geo.splitlines() if "Position:" in ln]
                        self.tl.mark("app.window", window=win, geometry="%dx%d+%s" % (w, h, (pos or ["?"])[0]))
                        return win
                    xdo(self.display, "windowsize", win, str(w), str(h))
                    time.sleep(0.5)
                raise SystemExit("window %s never mapped at %dx%d: %s" % (win, w, h, geo.strip()))
            time.sleep(1)
        raise SystemExit("no app window appeared (see %s/app.log)" % self.out)

    def focus(self):
        xdo(self.display, "windowraise", self.win)
        xdo(self.display, "windowfocus", "--sync", self.win)

    def key(self, *keys, to_window=True):
        """Menu accelerators need --window; webview keys go to the focus."""
        self.focus()
        if to_window:
            xdo(self.display, "key", "--window", self.win, *keys)
        else:
            xdo(self.display, "key", *keys)
        time.sleep(0.5)

    def click(self, x, y, repeat=1):
        xdo(self.display, "mousemove", "--window", self.win, str(x), str(y), "click", "--repeat", str(repeat), "1")
        time.sleep(0.8)

    def shot(self, name):
        path = os.path.join(self.out, name + ".png")
        r = subprocess.run(["import", "-display", self.display, "-window", self.win, path],
                           capture_output=True, timeout=60)
        if r.returncode != 0:
            raise SystemExit("capture failed: %s" % name)
        self.tl.mark("shot", file=path)
        return path

    def open_project(self):
        if not self.log.wait("log.open", self.launched - 1, 90):
            raise SystemExit("the app never opened its event log (%s)" % self.log.path)
        m = now_ms()
        for attempt in range(3):
            self.key("ctrl+o")
            e = self.log.wait("file.open", m, 12)
            if e:
                self.tl.mark("project.open", file=e["event"]["path"])
                return e["event"]["path"]
            print("no file.open after Ctrl+O (try %d)" % (attempt + 1), flush=True)
        raise SystemExit("Ctrl+O never opened the preset project")

    def open_tree_row(self, project, rows, name):
        m = now_ms()
        self.click(TREE_X, TREE_Y0 + TREE_DY * rows.index(name))
        want = os.path.join(project, name)
        e = self.log.wait("file.open", m, 15, lambda ev: ev.get("path") == want) or \
            self.log.wait("file.switch", m, 1, lambda ev: ev.get("path") == want)
        if not e:
            raise SystemExit("clicking the tree row for %s did not open it" % name)
        return want

    def stop(self):
        if self.p:
            self.procs.stop(self.p, grace=5)
            self.tl.mark("app.stop")
            self.p = None
