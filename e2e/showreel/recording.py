#!/usr/bin/env python3
"""recording.py — everything a take leaves for the edit, and the tools that
read it back. Times are seconds from the screen recording's start, matching
the per-take edit scripts, which stay in the workspace: only generic
transforms belong here."""
import json
import os
import subprocess


def say(msg):
    print("showreel: " + msg, flush=True)


def die(msg):
    raise SystemExit("showreel: FATAL " + msg)


def srt_time(ms):
    ms = max(0, int(ms))
    return "%02d:%02d:%02d,%03d" % (ms // 3600000, ms // 60000 % 60, ms // 1000 % 60, ms % 1000)


def write_captions(take_dir, beats, cap0):
    with open(os.path.join(take_dir, "captions.srt"), "w") as f:
        for n, b in enumerate(beats):
            start = b["start_ms"] - cap0
            f.write("%d\n%s --> %s\n%s\n\n" % (n + 1, srt_time(start), srt_time(start + 6000), b["prompt"]))


def preview(take_dir, speed):
    """A rough cut to judge a take by: the app and the terminal side by side,
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
    the first compile payoff after it, plus the final walk.
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
