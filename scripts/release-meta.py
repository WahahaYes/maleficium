#!/usr/bin/env python3
"""Release metadata shared by scripts/release.sh and the release workflow.

  release-meta.py check X.Y.Z   exit 1 unless package.json, Cargo.toml and
                                tauri.conf.json all carry X.Y.Z
  release-meta.py notes X.Y.Z   print the CHANGELOG.md section for X.Y.Z;
                                exit 1 when there is none
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def check(ver):
    bad = []
    if json.loads((ROOT / "package.json").read_text())["version"] != ver:
        bad.append("package.json")
    cargo = (ROOT / "src-tauri/Cargo.toml").read_text()
    if not re.search(r'^version = "%s"$' % re.escape(ver), cargo, re.M):
        bad.append("src-tauri/Cargo.toml")
    if json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())["version"] != ver:
        bad.append("src-tauri/tauri.conf.json")
    for f in bad:
        print("release-meta: %s is not at %s" % (f, ver), file=sys.stderr)
    return 1 if bad else 0


def notes(ver):
    text = (ROOT / "CHANGELOG.md").read_text()
    m = re.search(r"^## %s(?: [^\n]*)?\n(.*?)(?=^## |\Z)" % re.escape(ver), text, re.S | re.M)
    body = m.group(1).strip() if m else ""
    if not body:
        print("release-meta: no CHANGELOG section for %s" % ver, file=sys.stderr)
        return 1
    print(body)
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] not in ("check", "notes"):
        sys.exit(__doc__.strip())
    sys.exit({"check": check, "notes": notes}[sys.argv[1]](sys.argv[2]))
