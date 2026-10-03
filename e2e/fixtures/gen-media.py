#!/usr/bin/env python3
"""Regenerate the model and video fixtures of the interactive and playground papers.

Run from the repo root: python3 e2e/fixtures/gen-media.py

Provenance: both files are generated here, with no third-party asset, and are
dedicated to the public domain under CC0 1.0.
  models/mesh.glb  a 24-vertex colored cube as binary glTF 2.0 (about 2 KB),
                   written with the standard library only.
  media/clip.mp4   a 2 s, 160x90 H.264 test pattern from ffmpeg's built-in
                   `testsrc2` generator (needs ffmpeg with libx264).
Never write these through a text tool: the bytes are binary.
"""
import json
import struct
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
TARGETS = [HERE / "interactive", HERE / "playground"]

# One quad per face: (outward normal, color); the quad's corners follow from the normal.
FACES = [
    ((1, 0, 0), (0.85, 0.25, 0.25)),
    ((-1, 0, 0), (0.25, 0.65, 0.85)),
    ((0, 1, 0), (0.30, 0.75, 0.35)),
    ((0, -1, 0), (0.90, 0.75, 0.25)),
    ((0, 0, 1), (0.60, 0.35, 0.85)),
    ((0, 0, -1), (0.95, 0.55, 0.20)),
]


def cube() -> bytes:
    pos, nor, col, idx = [], [], [], []
    for n, c in FACES:
        axis = [i for i in range(3) if n[i] != 0][0]
        u, v = [i for i in range(3) if i != axis]
        base = len(pos) // 3
        for du, dv in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            p = [0.0, 0.0, 0.0]
            p[axis] = 0.5 * n[axis]
            p[u] = 0.5 * du
            p[v] = 0.5 * dv
            pos += p
            nor += [float(x) for x in n]
            col += list(c)
        # Counter-clockwise seen from outside: flip the winding on negative faces.
        order = (0, 1, 2, 0, 2, 3)
        if (n[axis] > 0) != (axis != 1):
            order = (0, 2, 1, 0, 3, 2)
        idx += [base + i for i in order]
    pb = struct.pack(f"<{len(pos)}f", *pos)
    nb = struct.pack(f"<{len(nor)}f", *nor)
    cb = struct.pack(f"<{len(col)}f", *col)
    ib = struct.pack(f"<{len(idx)}H", *idx)
    blob = pb + nb + cb + ib
    n = len(pos) // 3
    doc = {
        "asset": {"version": "2.0", "generator": "maleficium e2e gen-media.py"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1, "COLOR_0": 2}, "indices": 3, "material": 0}]}],
        "materials": [{"pbrMetallicRoughness": {"metallicFactor": 0.0, "roughnessFactor": 0.8}}],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": n, "type": "VEC3", "min": [-0.5] * 3, "max": [0.5] * 3},
            {"bufferView": 1, "componentType": 5126, "count": n, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5126, "count": n, "type": "VEC3"},
            {"bufferView": 3, "componentType": 5123, "count": len(idx), "type": "SCALAR"},
        ],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": len(pb), "target": 34962},
            {"buffer": 0, "byteOffset": len(pb), "byteLength": len(nb), "target": 34962},
            {"buffer": 0, "byteOffset": len(pb) + len(nb), "byteLength": len(cb), "target": 34962},
            {"buffer": 0, "byteOffset": len(pb) + len(nb) + len(cb), "byteLength": len(ib), "target": 34963},
        ],
        "buffers": [{"byteLength": len(blob)}],
    }
    js = json.dumps(doc, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    blob += b"\x00" * (-len(blob) % 4)
    total = 12 + 8 + len(js) + 8 + len(blob)
    return (
        struct.pack("<4sII", b"glTF", 2, total)
        + struct.pack("<I4s", len(js), b"JSON")
        + js
        + struct.pack("<I4s", len(blob), b"BIN\x00")
        + blob
    )


def main() -> None:
    glb = cube()
    for t in TARGETS:
        (t / "models").mkdir(exist_ok=True)
        (t / "models" / "mesh.glb").write_bytes(glb)
    clip = TARGETS[0] / "media" / "clip.mp4"
    clip.parent.mkdir(exist_ok=True)
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=160x90:rate=10:duration=2",
         "-c:v", "libx264", "-preset", "veryslow", "-crf", "34", "-pix_fmt", "yuv420p",
         "-movflags", "+faststart", "-map_metadata", "-1", "-fflags", "+bitexact", "-flags:v", "+bitexact", str(clip)],
        check=True,
    )
    for t in TARGETS[1:]:
        (t / "media").mkdir(exist_ok=True)
        (t / "media" / "clip.mp4").write_bytes(clip.read_bytes())


if __name__ == "__main__":
    main()
