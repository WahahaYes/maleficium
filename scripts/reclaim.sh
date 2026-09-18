#!/bin/sh
# Reclaim the vite/tauri dev ports held by this project's processes and
# clear stale engine orphans from hard-killed runs.
# Only processes whose working directory is inside this repo are
# signalled (TERM, then KILL after a grace period). Anything else holding
# a port is reported and left alone for its owner to free.
# POSIX sh (npm runs scripts under sh/dash). Always exits 0: a blocked
# port fails loudly at bind time with the hint below, never by surprise.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")" && pwd -P)

have() { command -v "$1" >/dev/null 2>&1; }

cwd_of() {
    if [ -r "/proc/$1/cwd" ]; then
        readlink "/proc/$1/cwd" 2>/dev/null || true
    else
        ps -p "$1" -o args= 2>/dev/null || true
    fi
}

is_ours() {
    # Fail-safe: cwd inside this repo means ours. A different project's
    # vite on the same port matches nothing here — reported, never killed.
    case "$1" in
        "$ROOT"*) return 0 ;;
        *) return 1 ;;
    esac
}

TERMED=""

reclaim_one() {
    port="$1"
    pids=""
    if have lsof; then
        pids=$(lsof -ti:"$port" 2>/dev/null || true)
    else
        echo "reclaim: lsof missing, cannot inspect :$port (skipping)" >&2
        return 0
    fi
    if [ -z "$pids" ]; then return 0; fi
    # shellcheck disable=SC2086
    for pid in $pids; do
        where=$(cwd_of "$pid")
        if [ -n "$where" ] && is_ours "$where"; then
            echo "reclaim: :$port held by ours (pid $pid, $where) — TERM"
            kill -TERM "$pid" 2>/dev/null || true
            TERMED="$TERMED $pid"
        else
            echo "reclaim: :$port held by UNRELATED pid $pid (${where:-unknown}) — leaving it; free :$port manually if the dev server fails to bind" >&2
        fi
    done
    return 0
}

reclaim_one 1420
reclaim_one 1421

if [ -n "$TERMED" ]; then
    sleep 2
    # shellcheck disable=SC2086
    for pid in $TERMED; do
        if kill -0 "$pid" 2>/dev/null; then
            where=$(cwd_of "$pid")
            if [ -n "$where" ] && is_ours "$where"; then
                echo "reclaim: $pid still alive — KILL"
                kill -KILL "$pid" 2>/dev/null || true
            fi
        fi
    done
fi

# Stale engine orphans. Scoped to the sidecar path fragment; the bracket
# trick keeps pkill from matching its own command line.
if have pkill; then
    pkill -f '[b]inaries/tectonic' 2>/dev/null || true
fi
exit 0
