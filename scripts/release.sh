#!/bin/sh
# Release in three steps, each ending where a person takes over. main is
# protected, so nothing here pushes to main or creates a tag.
#
#   release.sh prepare X.Y.Z [--notes-file FILE] [--no-push]
#       Cut (or update) branch release-X.Y.Z from origin/main in a scratch
#       worktree, bump every manifest to X.Y.Z, turn CHANGELOG.md's
#       Unreleased section into X.Y.Z (FILE replaces it instead), commit
#       "release X.Y.Z", push, and open the release PR. Rerunning updates the
#       same branch and PR. --no-push stops after the commit and keeps the
#       worktree for inspection.
#   release.sh draft X.Y.Z [--out DIR]
#       After the PR merges: run the release workflow on main, which builds
#       every platform and attaches the packages to a DRAFT GitHub release,
#       then download the Linux packages into DIR and smoke them. Publishing
#       the draft on GitHub (by hand) creates the vX.Y.Z tag.
#   release.sh build [--host] [--out DIR]
#       Build this checkout's Linux packages locally (Docker, or this host
#       with --host) to smoke before opening a release PR. Publishes nothing.
#
# POSIX sh. Needs git and python3; prepare also npm, cargo and gh; draft gh
# and docker; build docker (or a release-ready host with --host).
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)

usage() { sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
die() { printf 'release: %s\n' "$1" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }
version() {
    case "$1" in (*[!0-9.]*|""|.*|*.|*..*) usage;; esac
    case "$1" in ?*.?*.?*) ;; (*) usage;; esac
}
# The version a ref's package.json carries.
version_at() { git -C "$ROOT" show "$1:package.json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])'; }
tag_exists() { git -C "$ROOT" ls-remote --exit-code --tags origin "refs/tags/v$1" >/dev/null 2>&1; }

cmd_prepare() {
    VER="${1:-}"; shift || usage; version "$VER"
    NOTES_FILE=""; PUSH=1
    while [ $# -gt 0 ]; do
        case "$1" in
            --notes-file) NOTES_FILE="${2:-}"; shift 2 || usage ;;
            --no-push) PUSH=0; shift ;;
            *) usage ;;
        esac
    done
    need git; need python3; need npm; need cargo; need gh
    # The commit hooks run prettier from ./node_modules; the scratch worktree
    # borrows this checkout's.
    [ -d "$ROOT/node_modules" ] || die "no node_modules in $ROOT; run npm ci first"
    [ -z "$NOTES_FILE" ] || NOTES_FILE=$(cd -- "$(dirname -- "$NOTES_FILE")" && pwd -P)/$(basename -- "$NOTES_FILE")
    [ -z "$NOTES_FILE" ] || [ -s "$NOTES_FILE" ] || die "notes file $NOTES_FILE is missing or empty"

    BRANCH="release-$VER"
    git -C "$ROOT" fetch --quiet origin main
    tag_exists "$VER" && die "v$VER is already tagged; that version is released"
    [ "$(version_at origin/main)" != "$VER" ] || die "origin/main is already at $VER; run: sh scripts/release.sh draft $VER"

    # A detached scratch worktree: no local branch, so it works while
    # release-X.Y.Z is checked out elsewhere. Kept only by a successful
    # --no-push.
    WT=$(mktemp -d "${TMPDIR:-/tmp}/maleficium-$BRANCH-XXXXXX")
    KEEP=0
    cleanup() {
        [ "$KEEP" = 0 ] || return 0
        git -C "$ROOT" worktree remove --force "$WT" 2>/dev/null || rm -rf "$WT"
        git -C "$ROOT" worktree prune
    }
    trap cleanup EXIT
    # Rerun: continue the pushed branch, merging main in rather than rebasing.
    if git -C "$ROOT" fetch --quiet origin "refs/heads/$BRANCH:refs/remotes/origin/$BRANCH" 2>/dev/null; then
        git -C "$ROOT" worktree add --quiet --detach "$WT" "origin/$BRANCH"
        git -C "$WT" merge --quiet --no-edit origin/main
    else
        git -C "$ROOT" worktree add --quiet --detach "$WT" origin/main
    fi
    ln -s "$ROOT/node_modules" "$WT/node_modules"
    # This script's own release-meta.py, run against the worktree.
    meta() { RELEASE_ROOT="$WT" python3 "$ROOT/scripts/release-meta.py" "$@"; }

    # Bump the manifests. sed, not a JSON round-trip: loaders reformat
    # unrelated inline arrays. cargo metadata re-syncs Cargo.lock without a
    # build or the sidecars.
    (cd "$WT" && npm version --no-git-tag-version --allow-same-version "$VER" >/dev/null)
    sed -i.bak '0,/^version = "[^"]*"/s//version = "'"$VER"'"/' "$WT/src-tauri/Cargo.toml"
    sed -i.bak '0,/"version": "[^"]*"/s//"version": "'"$VER"'"/' "$WT/src-tauri/tauri.conf.json"
    rm -f "$WT/src-tauri/Cargo.toml.bak" "$WT/src-tauri/tauri.conf.json.bak"
    cargo metadata --manifest-path "$WT/src-tauri/Cargo.toml" --format-version 1 --offline >/dev/null 2>&1 \
        || cargo metadata --manifest-path "$WT/src-tauri/Cargo.toml" --format-version 1 >/dev/null
    meta check "$VER" || die "manifests not bumped to $VER"
    meta changelog "$VER" "$(date +%Y-%m-%d)" ${NOTES_FILE:+"$NOTES_FILE"} \
        || die "no CHANGELOG notes for $VER"
    meta stale \
        || die "a version is hardcoded outside the manifests; read it from the build instead"

    git -C "$WT" add package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock \
        src-tauri/tauri.conf.json CHANGELOG.md
    git -C "$WT" diff --cached --quiet || git -C "$WT" commit --quiet -m "release $VER"
    if [ "$PUSH" = 0 ]; then
        KEEP=1
        printf 'release: committed for %s in %s (not pushed; remove with git worktree remove)\n' "$BRANCH" "$WT"
        git -C "$WT" log --oneline origin/main..
        return
    fi
    git -C "$WT" push --quiet origin "HEAD:refs/heads/$BRANCH"
    PR=$(gh pr list --head "$BRANCH" --state open --json url --jq '.[0].url // empty')
    if [ -n "$PR" ]; then
        printf 'release: updated %s\n' "$PR"
    else
        # shellcheck disable=SC2016 # the backticks are Markdown
        BODY=$(printf 'Bumps every manifest to %s and turns its CHANGELOG section into the release notes the release workflow publishes.\n\nAfter merge: `sh scripts/release.sh draft %s` builds every platform into a draft release and smokes the Linux packages. Nothing is published or tagged until the draft is published by hand.\n\n## Release notes\n\n%s\n' \
            "$VER" "$VER" "$(meta notes "$VER")")
        gh pr create --base main --head "$BRANCH" --title "release $VER" --body "$BODY"
    fi
}

cmd_draft() {
    VER="${1:-}"; shift || usage; version "$VER"
    OUT="$ROOT/../maleficium-release-$VER"
    while [ $# -gt 0 ]; do
        case "$1" in
            --out) OUT="${2:-}"; shift 2 || usage ;;
            *) usage ;;
        esac
    done
    need git; need python3; need gh; need docker
    git -C "$ROOT" fetch --quiet origin main
    [ "$(version_at origin/main)" = "$VER" ] \
        || die "origin/main is at $(version_at origin/main), not $VER; merge the release PR first"
    tag_exists "$VER" && die "v$VER is already tagged; that version is published"
    [ ! -e "$OUT" ] || [ -z "$(ls -A "$OUT")" ] || die "$OUT is not empty; pass a fresh --out"

    SINCE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    gh workflow run release.yml --ref main -f version="$VER"
    # The dispatch returns no run id: find the run by its name and start time
    # ("release" alone before release.yml had a run-name).
    RUN=""; i=0
    while [ -z "$RUN" ] && [ $i -lt 30 ]; do
        sleep 2; i=$((i + 1))
        RUN=$(gh run list --workflow release.yml --event workflow_dispatch --limit 10 \
            --json databaseId,displayTitle,createdAt \
            --jq "[.[] | select((.displayTitle == \"release $VER\" or .displayTitle == \"release\") and .createdAt >= \"$SINCE\")][0].databaseId // empty")
    done
    [ -n "$RUN" ] || die "dispatched, but no release run for $VER appeared; check the Actions tab"
    printf 'release: watching run %s\n' "$RUN"
    gh run watch "$RUN" --exit-status --interval 30 >/dev/null || die "release run $RUN failed: gh run view $RUN --log-failed"

    mkdir -p "$OUT"
    gh run download "$RUN" --pattern 'linux-*' --dir "$OUT"
    # One matching artifact still lands in its own folder; flatten it.
    find "$OUT" -mindepth 2 -type f -exec mv {} "$OUT" \;
    find "$OUT" -mindepth 1 -type d -empty -delete
    for f in "$OUT"/*; do
        case "${f##*/}" in (*_"$VER"_*) ;; (*) die "$f is not a $VER package" ;; esac
    done
    python3 "$ROOT/e2e/package-smoke.py" "$OUT" || die "smoke failed on $OUT; do not publish the draft"
    URL=$(gh release view "v$VER" --json url --jq .url)
    printf 'release: draft v%s is ready and its Linux packages pass the smoke: %s\n' "$VER" "$URL"
    printf 'release: publishing the draft on GitHub creates the v%s tag.\n' "$VER"
}

cmd_build() {
    HOST=0; OUT="$ROOT/../maleficium-release"
    while [ $# -gt 0 ]; do
        case "$1" in
            --host) HOST=1; shift ;;
            --out) OUT="${2:-}"; shift 2 || usage ;;
            *) usage ;;
        esac
    done
    mkdir -p "$OUT"
    if [ "$HOST" = 1 ]; then
        sh "$ROOT/scripts/package.sh" --out "$OUT"
    else
        need docker
        docker build --output "type=local,dest=$OUT" "$ROOT"
    fi
    printf 'release: packages in %s; smoke them with python3 e2e/package-smoke.py %s\n' "$OUT" "$OUT"
}

CMD="${1:-}"; shift || usage
case "$CMD" in
    prepare) cmd_prepare "$@" ;;
    draft) cmd_draft "$@" ;;
    build) cmd_build "$@" ;;
    *) usage ;;
esac
