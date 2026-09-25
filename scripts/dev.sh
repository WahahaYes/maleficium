#!/bin/sh
# Run a dev loop on the first free dev port pair, so parallel instances
# (a second checkout, a worktree) never collide. The stills harness pins
# 1420 itself and does not come through here.
#   sh scripts/dev.sh web      — vite only
#   sh scripts/dev.sh desktop  — tauri dev, devUrl pointed at the same port
# A pair is P (vite) and P+1 (HMR socket under TAURI_DEV_HOST), P from
# 1420 upward in steps of 2. DEV_PORT, if already set, is used as-is.
# vite.config.ts binds DEV_PORT with strictPort, so vite and devUrl can
# never disagree: a race for the same pair fails loudly at bind time.
# POSIX sh (npm runs scripts under sh/dash).
set -eu
unset CDPATH

MODE="${1:-}"
[ $# -gt 0 ] && shift

free() {
    # Bind the dual-stack wildcard: fails if anything listens on the port
    # on any address, whoever owns it.
    node -e 'const s = require("net").createServer();
s.once("error", () => process.exit(1));
s.listen(+process.argv[1], () => s.close(() => process.exit(0)));' "$1"
}

pick() {
    p=1420
    while [ "$p" -le 1438 ]; do
        if free "$p" && free $((p + 1)); then
            echo "$p"
            return 0
        fi
        p=$((p + 2))
    done
    echo "dev: no free dev port pair in 1420-1439" >&2
    return 1
}

if [ -z "${DEV_PORT:-}" ]; then DEV_PORT=$(pick); fi
export DEV_PORT
echo "dev: port $DEV_PORT"

case "$MODE" in
    web) exec vite "$@" ;;
    desktop)
        # Tauri shells out to cargo; a shell without it dies inside the
        # CLI with an inscrutable metadata error. Recover from the stock
        # rustup env when present, else fail with a pointer.
        if ! command -v cargo >/dev/null 2>&1; then
            for env in "${HOME:-/nonexistent}/.cargo/env" /usr/local/cargo/env; do
                if [ -f "$env" ]; then . "$env"; break; fi
            done
        fi
        command -v cargo >/dev/null 2>&1 || {
            echo "dev: cargo not found; install Rust 1.98.1+ or add cargo to PATH (see BUILDING.md)" >&2
            exit 1
        }
        exec tauri dev --config "{\"build\":{\"devUrl\":\"http://localhost:$DEV_PORT\"}}" "$@" ;;
    *)
        echo "dev: usage: dev.sh web|desktop [args...]" >&2
        exit 1
        ;;
esac
