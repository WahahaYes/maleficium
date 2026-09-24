#!/bin/sh
# Reclaim dev ports held by orphans of this project's hard-killed runs
# and clear orphaned engine sidecars. Live dev sessions are never touched:
# parallel instances each hold their own port pair (scripts/dev.sh).
# A port holder is signalled (TERM, then KILL after a grace period) only
# if its working directory is inside this repo AND it is an orphan: the
# topmost ancestor still inside this repo was reparented to init or a
# systemd subreaper, i.e. the launcher that owned it (npm, tauri dev, the
# shell) is gone. Anything else holding a port is reported and left alone.
# POSIX sh (npm runs scripts under sh/dash). Always exits 0: a blocked
# port fails loudly at bind time with the hint below, never by surprise.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)

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

parent_of() { ps -o ppid= -p "$1" 2>/dev/null | tr -d ' '; }

reaper() {
    # $1 = pid. True for init and systemd (user-manager subreapers).
    [ "$1" = 1 ] && return 0
    [ "$(ps -o comm= -p "$1" 2>/dev/null)" = systemd ]
}

is_orphan() {
    # $1 = pid known to be ours. Climb while the parent is still ours.
    # Underscored names: POSIX sh has no locals and callers hold pid/where.
    _o="$1"
    while :; do
        _p=$(parent_of "$_o")
        [ -n "$_p" ] || return 1
        reaper "$_p" && return 0
        _w=$(cwd_of "$_p")
        if [ -n "$_w" ] && is_ours "$_w"; then
            _o="$_p"
        else
            return 1
        fi
    done
}

TERMED=""

reclaim_one() {
    port="$1"
    pids=""
    if have lsof; then
        pids=$(lsof -ti tcp:"$port" -sTCP:LISTEN 2>/dev/null || true)
    else
        echo "reclaim: lsof missing, cannot inspect :$port (skipping)" >&2
        return 0
    fi
    if [ -z "$pids" ]; then return 0; fi
    # shellcheck disable=SC2086
    for pid in $pids; do
        where=$(cwd_of "$pid")
        if [ -n "$where" ] && is_ours "$where"; then
            if is_orphan "$pid"; then
                echo "reclaim: :$port held by our orphan (pid $pid, $where) — TERM"
                kill -TERM "$pid" 2>/dev/null || true
                TERMED="$TERMED $pid"
            fi
        else
            echo "reclaim: :$port held by UNRELATED pid $pid (${where:-unknown}) — leaving it; free :$port manually if the dev server fails to bind" >&2
        fi
    done
    return 0
}

# The whole range scripts/dev.sh picks from.
port=1420
while [ "$port" -le 1439 ]; do
    reclaim_one "$port"
    port=$((port + 1))
done

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

# Engine orphans: a sidecar whose app died is reparented to a reaper. A
# live instance's sidecar keeps its app as parent and is left alone.
# Scoped to the sidecar path fragment; the bracket trick keeps pgrep from
# matching its own command line.
if have pgrep; then
    for pid in $(pgrep -f '[b]inaries/maleficium-tectonic' 2>/dev/null || true); do
        ppid=$(parent_of "$pid")
        if [ -n "$ppid" ] && reaper "$ppid"; then
            echo "reclaim: orphaned engine pid $pid — KILL"
            kill -KILL "$pid" 2>/dev/null || true
        fi
    done
fi
exit 0
