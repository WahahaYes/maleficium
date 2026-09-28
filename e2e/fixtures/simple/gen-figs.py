#!/usr/bin/env python3
"""Regenerate e2e/fixtures/simple/figs/*.png (valid 64x64 RGB, zlib-crafted, no assets).

Run with: python3 e2e/fixtures/simple/gen-figs.py (from the repo root).
IMPORTANT: never write these PNGs through a text tool or editor — binary
bytes mangled as UTF-8 corrupt the signature (this broke a compile once:
`Unable to load picture or PDF file 'figs/diagram.png'`). If `file figs/*.png`
ever reports `data` instead of `PNG image data`, rerun this script.
"""
import struct
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIGS = HERE / "figs"


def chunk(ctype: bytes, data: bytes) -> bytes:
    c = struct.pack(">I", len(data)) + ctype + data
    return c + struct.pack(">I", zlib.crc32(ctype + data) & 0xFFFFFFFF)


def png(w: int, h: int, rgb: tuple) -> bytes:
    sig = bytes([137, 80, 78, 71, 13, 10, 26, 10])
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def main() -> None:
    FIGS.mkdir(parents=True, exist_ok=True)
    targets = {"diagram.png": (70, 130, 200), "photo.png": (200, 120, 60)}
    for name, rgb in targets.items():
        p = FIGS / name
        p.write_bytes(png(64, 64, rgb))
        d = p.read_bytes()
        assert d[:8] == bytes([137, 80, 78, 71, 13, 10, 26, 10]), f"bad signature: {name}"
        print(f"{name}: {len(d)}B signature ok")


if __name__ == "__main__":
    main()
