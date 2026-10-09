"""Digitize figures/e1e2.png (the published E1/E2 plot) into data/e1e2.csv.

The raw trial logs are not available, so every value here is read from the
figure's pixels: axis calibration from the plot's own gridlines, one sample
per integer context size. Run: python3 -I tools/digitize_e1e2.py
"""
import csv
import os
import sys

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
im = np.array(Image.open(os.path.join(ROOT, "figures/e1e2.png")).convert("RGB")).astype(int)

# (panel, x of tick 0 and 10, y of 0% and of the top gridline with its value)
PANELS = {
    "E1": dict(x0=217, x10=1478, y0=973, ytop=214, vtop=40.0),
    "E2": dict(x0=1667, x10=2929, y0=973, ytop=241, vtop=60.0),
}
SERIES = {
    "vlm": lambda p: (p[..., 2] > 200) & (p[..., 0] < 60) & (p[..., 1] < 60),
    "greedy": lambda p: (p[..., 0] > 230) & (p[..., 1] > 180) & (p[..., 1] < 225) & (p[..., 2] < 60),
    "random_prior": lambda p: (p[..., 1] > 110) & (p[..., 0] < 60) & (p[..., 2] < 60),
    "random_visible": lambda p: (p[..., 0] > 230) & (p[..., 1] < 50) & (p[..., 2] < 50),
}
# The CI band is a translucent blue fill; the VLM line runs inside it.
BAND = lambda p: (abs(p[..., 0] - p[..., 1]) < 12) & (p[..., 2] > 235) & (p[..., 0] > 130) & (p[..., 0] < 175)


def value(cfg, y):
    return (cfg["y0"] - y) * cfg["vtop"] / (cfg["y0"] - cfg["ytop"])


def column_rows(mask, x, half):
    sub = mask[:, max(0, x - half) : x + half + 1]
    return np.where(sub.any(1))[0]


def centre(rows):
    if len(rows) == 0:
        return None
    groups = np.split(rows, np.where(np.diff(rows) > 3)[0] + 1)
    best = max(groups, key=len)
    return float(np.mean(best))


MASKS = {name: pred(im) for name, pred in SERIES.items()}
# Lines drawn over the band count as band, or a crossing would split it.
MASKS["band"] = BAND(im) | MASKS["vlm"] | MASKS["greedy"] | MASKS["random_prior"]
# Values the paper states in its text replace the pixel readings.
STATED = {("E1", 0): (10.3, 8.3, 12.3), ("E1", 6): (24.8, 22.1, 27.7), ("E2", 6): (49.5, 43.0, 56.1)}
rows_out = []
for panel, cfg in PANELS.items():
    step = (cfg["x10"] - cfg["x0"]) / 10
    for k in range(11):
        x = int(round(cfg["x0"] + k * step))
        rec = {"experiment": panel, "fixations": k}
        lo = x - (0 if k == 0 else 30)
        hi = x + (0 if k == 10 else 30)
        for name, pred in SERIES.items():
            mask = MASKS[name]
            if name == "vlm":
                c = centre(column_rows(mask, x, 2))
                rec[name] = None if c is None else round(value(cfg, c), 2)
                continue
            # Dashes leave gaps: fit a line through the dash pixels near the tick.
            pts = [(xx, centre(column_rows(mask, xx, 0))) for xx in range(lo, hi + 1)]
            pts = [(xx, c) for xx, c in pts if c is not None]
            if len(pts) < 4 or min(abs(xx - x) for xx, _ in pts) > 25:
                rec[name] = None
                continue
            a, b = np.polyfit([q[0] for q in pts], [q[1] for q in pts], 1)
            rec[name] = round(value(cfg, a * x + b), 2)
        # The band's edges, as the median over nearby columns, so a line
        # crossing one column does not move them.
        tops, bots = [], []
        # At the plot's ends the band is a wedge cut by the frame: read just inside.
        span = {0: range(x + 2, x + 6), 10: range(x - 5, x - 1)}.get(k, range(x - 12, x + 13))
        for xx in span:
            r = np.where(MASKS["band"][:, xx])[0]
            if not len(r):
                continue
            groups = np.split(r, np.where(np.diff(r) > 2)[0] + 1)
            g = max(groups, key=len)
            tops.append(g[0])
            bots.append(g[-1])
        if tops:
            rec["ci_high"] = round(value(cfg, float(np.median(tops))), 2)
            rec["ci_low"] = round(value(cfg, float(np.median(bots))), 2)
        exact = STATED.get((panel, k))
        rec["source"] = "figure"
        if exact:
            rec["vlm"], rec["ci_low"], rec["ci_high"] = exact
            rec["source"] = "text"
        rows_out.append(rec)

fields = ["experiment", "fixations", "vlm", "ci_low", "ci_high", "greedy", "random_prior", "random_visible", "source"]
os.makedirs(os.path.join(ROOT, "data"), exist_ok=True)
with open(os.path.join(ROOT, "data/e1e2.csv"), "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=fields, lineterminator="\n")
    w.writeheader()
    for r in rows_out:
        w.writerow({k: ("" if r.get(k) is None else r[k]) for k in fields})
w = csv.DictWriter(sys.stdout, fieldnames=fields)
w.writeheader()
for r in rows_out:
    w.writerow({k: r.get(k) for k in fields})
