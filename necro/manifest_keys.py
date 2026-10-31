#!/usr/bin/env python3
"""necro helper: runtime.json manifests are a closed schema.

Prints one `path:1` hit per unknown top-level key in every runtime.json
under embed-runtime/. Canonical enforcement is ManifestFile's
deny_unknown_fields (src-tauri/core/src/runtimes.rs); the legal field list
is docs/RUNTIMES.md. `poster` is legal on built-ins (embed-runtime/src/)
only, which use contract 2.
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

BASE_KEYS = {
    "contract",
    "name",
    "version",
    "title",
    "description",
    "authors",
    "license",
    "sources",
    "options",
    "capabilities",
    "vendored",
}
BUILTIN_KEYS = {"poster"}


def main() -> int:
    hits = []
    for path in sorted((ROOT / "embed-runtime").rglob("runtime.json")):
        rel = path.relative_to(ROOT).as_posix()
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            hits.append(f"{rel}:1: unreadable manifest: {exc}")
            continue
        if not isinstance(data, dict):
            hits.append(f"{rel}:1: manifest is not an object")
            continue
        allowed = BASE_KEYS | (BUILTIN_KEYS if rel.startswith("embed-runtime/src/") else set())
        for key in sorted(set(data) - allowed):
            hits.append(f"{rel}:1: unknown top-level key `{key}`")
    for hit in hits:
        print(hit)
    return 0


if __name__ == "__main__":
    sys.exit(main())
