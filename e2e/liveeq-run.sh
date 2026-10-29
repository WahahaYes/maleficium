#!/bin/bash
# The live-equation formula language through the package's l3fp path: every
# shared case in embed-runtime/src/expr.vectors.json, in one engine compile
# (e2e/liveeq-vectors.py; vitest runs the same cases on the reader's
# interpreter). Needs the engine from scripts/build-engine.sh and python3;
# network on a cold TeX cache (LIVEEQ_CACHE).
set -euo pipefail
DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec python3 "$DEVROOT/e2e/liveeq-vectors.py"
