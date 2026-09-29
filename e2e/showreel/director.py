#!/usr/bin/env python3
"""director.py — the showreel camera.

It follows the agent in the app. It reads tool calls and results off the
agent's transcript (as Claude-shaped message envelopes, whatever runner
produced them) and turns them into camera moves, one at a time, sent
through the app's dev camera channel: it sends no keystrokes, so it can
never change a document. A write pre-positions the camera while the tool
runs, then holds the changed line while the app reloads it, so the edit
lands on screen; holds shorten while the camera is behind, and a
compile's forward-sync yields to newer edits.
"""
import difflib
import json
import os
import queue
import threading
import time

from harness import now_ms, xdo


class Director(threading.Thread):
    def __init__(self, app, project, log_path, active, camera_file, display, screen, dwell=1.2, debug=False):
        super().__init__(daemon=True)
        self.app, self.project, self.log_path = app, project, log_path
        self.camera_file, self.display, self.screen = camera_file, display, screen
        self.dwell_s, self.debug = dwell, debug
        self.q = queue.Queue()
        self.uses = {}
        self.seen = {}
        # The file the app shows: opening it again raises no event.
        self.current = active
        self.last_line = {}
        self.halt = threading.Event()
        self.shots = 0
        self.seq = 0
        open(camera_file, "w").close()  # a fresh app starts at move 1
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
        if self.debug and self.app.win:
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
        return 0.6 if self.q.qsize() > 2 else self.dwell_s

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
    # (src/lib/devCamera.ts): numbered moves appended to the camera file,
    # run by the app's own handlers. No keystrokes, so no move can change a
    # document. The only input device the camera uses is the mouse wheel.

    def send(self, op, **fields):
        self.seq += 1
        with open(self.camera_file, "a") as f:
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
        w, h = (int(x) for x in self.screen.split("x"))
        xdo(self.display, "mousemove", "--window", self.app.win, str(int(w * 0.83)), str(h // 2))
        for _ in range(max(1, pages) * 12):
            xdo(self.display, "click", "5")
            time.sleep(0.15)
        time.sleep(2)
