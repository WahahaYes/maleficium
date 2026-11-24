"""Draw the hand-made posters of the playground paper (figures/*.png).

Run from the repo root: python3 e2e/fixtures/playground/tools/chart_poster.py

Provenance: both files are drawn here with Pillow, with no third-party
asset, and are dedicated to the public domain under CC0 1.0.
  figures/chart-poster.png   bars of accuracy by method, the document's own
                             poster for the styled chart widget (which keeps
                             charts/ablation.vl.json's data).
  figures/bridge-poster.png  the bridge widget's swatches, its poster.
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
FIG = HERE.parent / "figures"

ACCURACY = [
    ("baseline", 0.71),
    ("ours-small", 0.79),
    ("ours-base", 0.84),
    ("ours-large", 0.88),
    ("ours-xl", 0.89),
]
# The house light plate, ink and accent the paper typesets with.
BG = (255, 255, 255)
SURFACE = (241, 239, 233)
INK = (30, 27, 36)
ACCENT = (91, 42, 134)
CHIPS = [ACCENT, (42, 120, 214), (235, 104, 52), (27, 175, 122), (237, 161, 0)]


def chart():
    img = Image.new("RGB", (640, 360), BG)
    d = ImageDraw.Draw(img)
    font = ImageFont.load_default()
    left, bottom, top, = 56, 312, 24
    width = 640 - left - 16
    d.line([left, top, left, bottom], fill=INK)
    d.line([left, bottom, 640 - 16, bottom], fill=INK)
    slot = width / len(ACCURACY)
    for i, (name, value) in enumerate(ACCURACY):
        bar_w = slot * 0.6
        x0 = left + slot * i + (slot - bar_w) / 2
        h = (bottom - top - 24) * value
        d.rectangle([x0, bottom - h, x0 + bar_w, bottom], fill=ACCENT)
        d.text((x0, bottom + 6), name, fill=INK, font=font)
        d.text((x0, bottom - h - 12), f"{value:.2f}", fill=INK, font=font)
    img.save(FIG / "chart-poster.png")


def bridge():
    img = Image.new("RGB", (640, 200), BG)
    d = ImageDraw.Draw(img)
    font = ImageFont.load_default()
    d.rectangle([16, 16, 624, 72], fill=SURFACE)
    d.text((28, 34), "the bridge is up", fill=INK, font=font)
    for i, color in enumerate(CHIPS):
        d.rectangle([16 + i * 124, 88, 16 + i * 124 + 116, 152], fill=color)
    d.text((16, 166), "accent, then four palette steps", fill=INK, font=font)
    img.save(FIG / "bridge-poster.png")


chart()
bridge()
