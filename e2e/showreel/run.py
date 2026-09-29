#!/usr/bin/env python3
"""run.py (e2e/showreel/) — record an agent writing a document in the open app, for a video.

The agent works live and unassisted through a scripted conversation: one
agent session, resumed for each beat of the scenario
(e2e/showreel/scenarios/*.json). Agents live in agents.py: claude -p,
interactive Claude Code in tmux on camera, or headless opencode. The harness never edits the
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
  python3 e2e/showreel/run.py [--scenario coffee] [-n N] [--model M] [--beats N]
                              [--out DIR] [--budget USD] [--no-build]
Env: SHOWREEL_DISPLAY (:96), SHOWREEL_SCREEN (1536x864: upscale to 1080p in
the edit for a larger UI), SHOWREEL_PORT (1424), SHOWREEL_MIRROR_PORT
(18792), SHOWREEL_HOME (/tmp/barista), SHOWREEL_DEBUG=1 (a screenshot per camera
move), CARGO_TARGET_DIR (default
/var/tmp/maleficium-showreel-target).
Manual only: it spends model credits.
"""
import argparse, importlib.util, json, os, re, shutil, signal, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.dirname(HERE))
import harness
from agents import ClaudePrint, OpencodePrint, TmuxClaude, start_term_display
from director import Director
import recording

_spec = importlib.util.spec_from_file_location("agent_run", os.path.join(os.path.dirname(HERE), "agent-run.py"))
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


# ---- a take -----------------------------------------------------------------------


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


def beat_oracles(beat, prev_texts):
    """A beat's dict-oracles for the Judge. The two sequential forms are
    translated to absolute thresholds against the previous beat's texts."""
    out = []
    for o in beat["oracles"]:
        (kind, arg), = o.items()
        if kind == "more_addplots_than_before":
            n = sum(len(re.findall(r"\\addplot", t)) for t in prev_texts.values())
            out.append({"count_at_least": {"pattern": "\\\\addplot", "n": n + 1}})
        elif kind == "table_columns_grew":
            w = max([ar.max_tabular_width(t) for t in prev_texts.values()] + [0])
            out.append({"table_columns_at_least": {"n": w + 1}})
        else:
            out.append(o)
    return out


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
    director = Director(app, project, os.path.join(take_dir, "camera.jsonl"), s["main"],
                        CAMERA_FILE, DISP, SCREEN, DWELL, DEBUG)
    events_path = os.path.join(take_dir, "events.jsonl")
    common = (ar, server, model, events_path, tool_names, director.on_event, AGENT_HOME, MCP_CONFIG, HOME)
    if agent == "tmux":
        session = TmuxClaude(*common, procs=procs, take_dir=take_dir)
    elif agent == "opencode":
        session = OpencodePrint(*common)
    else:
        session = ClaudePrint(*common, budget_usd=budget)
    capture, beats = None, []
    try:
        app.launch(project)
        app.place()
        app.open_project()
        if not app.log.wait("compile.warm-skipped", app.launched - 1, 30):
            say("no warm-skipped event; continuing")
        director.send("command", id="view.toggle-log")  # hide the log pane (it shows full paths)
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
        session.stop()
        if agent == "tmux":
            tl.mark("capture.stop", video="claude.mp4")
        if capture:
            procs.stop(capture, sig=signal.SIGINT, grace=20)
            tl.mark("capture.stop")
        app.stop()

    recording.write_captions(take_dir, beats, cap0)

    # The take's own engine cache: it holds whatever the agent fetched.
    judge_home = os.path.join(take_dir, "judge-home")
    ar.seed_home(HOME, judge_home)
    prev = {rel: t for rel, t in s["files"].items() if rel.endswith(".tex")}
    for i, b in enumerate(beats):
        judged = ar.Judge({"id": b["id"], "oracles": beat_oracles(s["beats"][i], prev)},
                          server, b["snapshot"], judge_home, {}, []).run()
        b["oracles"] = judged
        b["passed"] = all(o["ok"] for o in judged) and b["exit"] == 0 and not b["timed_out"]
        prev = ar.project_texts(b["snapshot"])
    shutil.rmtree(judge_home, ignore_errors=True)
    passed = len(beats) == min(len(s["beats"]), max_beats) and all(b["passed"] for b in beats)
    tl.mark("take.end", passed=passed)
    return {"scenario": s["id"], "model": model, "passed": passed, "cost": round(session.cost, 4),
            "beats": beats, "timeline": tl.marks}


def camera_test(s, server, warm_home, out, procs, preset, bundle_url, fixture):
    """No agent: the camera's moves on a finished paper, a screenshot after each."""
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
    d = Director(app, project, os.path.join(out, "camera.jsonl"), s["main"],
                 CAMERA_FILE, DISP, SCREEN, DWELL, True)
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


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--scenario", default="coffee")
    ap.add_argument("-n", type=int, default=1, help="takes")
    ap.add_argument("--model", help="default: the claude model for tmux/print, the free model for opencode")
    ap.add_argument("--out", help="default /var/tmp/maleficium-showreel/<stamp>")
    ap.add_argument("--budget", type=float, default=10.0, help="stop starting takes once spend reaches this")
    ap.add_argument("--beat-budget", type=float, default=3.0, help="claude --max-budget-usd per beat")
    ap.add_argument("--beats", type=int, default=99, help="stop after this many beats (rehearsals)")
    ap.add_argument("--agent", default="tmux", choices=["tmux", "print", "opencode"],
                    help="tmux: interactive Claude Code on camera (default); "
                         "print: claude -p, off camera; opencode: headless opencode, off camera")
    ap.add_argument("--no-build", action="store_true")
    ap.add_argument("--preview", metavar="TAKE_DIR",
                    help="stitch a take's two recordings side by side with captions, sped up, and exit")
    ap.add_argument("--suggest-shots", metavar="TAKE_DIR",
                    help="print candidate edit shots from a take's logs and exit")
    ap.add_argument("--speed", type=float, default=6.0, help="speed-up for --preview")
    ap.add_argument("--camera-test", metavar="PROJECT",
                    help="no agent: run the camera's moves on a finished project, with screenshots")
    a = ap.parse_args()
    model = a.model or (ar.FREE_MODEL if a.agent == "opencode" else ar.CLAUDE_MODEL)
    if a.preview:
        return recording.preview(a.preview, a.speed)
    if a.suggest_shots:
        return recording.suggest_shots(a.suggest_shots)
    tools = ["Xvfb", "xdotool", "import", "bwrap", "curl", "ffmpeg"]
    if a.agent == "tmux":
        tools += ["tmux", "xterm", "claude"]
    elif a.agent == "print":
        tools += ["claude"]
    else:
        tools += ["opencode"]
    for tool in tools:
        if not shutil.which(tool):
            die("needs %s on PATH" % tool)
    if not a.camera_test and a.agent in ("tmux", "print"):
        ar.claude_token()
    with open(os.path.join(HERE, "scenarios", a.scenario + ".json")) as f:
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
        harness.start_mirror(procs, out, os.path.dirname(HERE), MIRROR_PORT, ar.MIRROR_CACHE)
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
            say("take %d (%s, %s) ..." % (i + 1, a.agent, model))
            res = one_take(s, server, warm_home, model, take_dir, procs, preset, bundle_url, tool_names,
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
