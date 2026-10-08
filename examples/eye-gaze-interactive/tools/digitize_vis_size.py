"""Digitize figures/vis_size.png (the published accuracy-requirement curves)
into data/vis_size.csv.

The per-object measurements are not available, so the curves are read from
the figure: for each measurement panel and interaction space, the visual
size (degrees) at each coverage percentile from 95% to 5%. Values above the
plot's 12 degree ceiling are left empty. Run: python3 -I tools/digitize_vis_size.py
"""
import csv
import os

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
im = np.array(Image.open(os.path.join(ROOT, "figures/vis_size.png")).convert("RGB")).astype(int)

Y0, Y12 = 1012, 127  # pixel rows of 0 and 12 degrees
PANELS = {"radius": (349, 875), "minor": (1661, 2187)}  # x of 95% and 50%
COLORS = {
    "near": (100, 143, 255),
    "mid": (255, 176, 0),
    "interacted": (254, 97, 0),
    "fixated": (220, 38, 127),
}


def mask_for(rgb):
    d = np.abs(im - np.array(rgb)).sum(2)
    return d < 60


MASKS = {k: mask_for(v) for k, v in COLORS.items()}


def deg(y):
    return (Y0 - y) * 12.0 / (Y0 - Y12)


rows = []
for measure, (x95, x50) in PANELS.items():
    per = (x50 - x95) / 45.0
    for space, mask in MASKS.items():
        prev = None
        for pct in range(95, 4, -1):
            x = int(round(x50 + (50 - pct) * per))
            r = np.where(mask[Y12 - 2 : Y0 + 2, x - 1 : x + 2].any(1))[0] + Y12 - 2
            val = None
            if len(r):
                groups = np.split(r, np.where(np.diff(r) > 3)[0] + 1)
                # Left of 50% the dashed marker line shares the colour and sits
                # above the curve: the curve is the lowest run.
                g = groups[-1]
                val = round(deg(float(np.mean(g))), 3)
                if prev is not None and val < prev - 0.3:
                    val = None  # another curve's antialiasing, not this one
            rows.append({"measure": measure, "space": space, "pct": pct, "deg": val})
            if val is not None:
                prev = val
        # Fill single-column gaps (occlusion by another curve) linearly.
        seq = [r for r in rows if r["measure"] == measure and r["space"] == space]
        for i in range(1, len(seq) - 1):
            if seq[i]["deg"] is None:
                a = next((seq[j] for j in range(i - 1, -1, -1) if seq[j]["deg"] is not None), None)
                b = next((seq[j] for j in range(i + 1, len(seq)) if seq[j]["deg"] is not None), None)
                if a and b and b["pct"] - a["pct"] >= -6 and b["deg"] < 11.9:
                    t = (seq[i]["pct"] - a["pct"]) / (b["pct"] - a["pct"])
                    seq[i]["deg"] = round(a["deg"] + t * (b["deg"] - a["deg"]), 3)

os.makedirs(os.path.join(ROOT, "data"), exist_ok=True)
with open(os.path.join(ROOT, "data/vis_size.csv"), "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=["measure", "space", "pct", "deg"], lineterminator="\n")
    w.writeheader()
    for r in rows:
        if r["deg"] is not None:
            w.writerow(r)
for measure in PANELS:
    for space in COLORS:
        at = {r["pct"]: r["deg"] for r in rows if r["measure"] == measure and r["space"] == space}
        print(measure, space, "95:", at[95], "75:", at[75], "50:", at[50], "25:", at[25], "n=", sum(v is not None for v in at.values()))
