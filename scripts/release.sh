#!/bin/sh
# Cut a release in one command. main and v* tags are protected, so it goes
# the way any change does: a PR that must pass the required builds.
#
#   release.sh X.Y.Z [--notes-file FILE] [--no-push]
#       1. Cut (or update) branch release-X.Y.Z from origin/main in a scratch
#          worktree: bump every manifest to X.Y.Z, turn CHANGELOG.md's
#          Unreleased section into X.Y.Z (FILE replaces it instead), commit
#          "release X.Y.Z", push, open the release PR.
#       2. Wait for the PR's required builds, then merge it.
#       3. Tag the merge commit vX.Y.Z and push the tag (as you: only
#          repository admins may create v* tags).
#       4. The tag runs release.yml, which builds every platform again and
#          publishes the GitHub release. Wait for it and print its URL.
#       Rerunning resumes: an open PR is updated, a merged release is tagged,
#       a pushed tag is watched. --no-push stops after the commit and keeps
#       the worktree for inspection.
#   release.sh build [--host] [--out DIR]
#       Build this checkout's Linux packages locally (Docker, or this host
#       with --host) to smoke before a release. Publishes nothing.
#
# POSIX sh. Needs git, python3, npm, cargo and gh; build needs docker (or a
# release-ready host with --host).
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)

usage() { sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
die() { printf 'release: %s\n' "$1" >&2; exit 1; }
say() { printf 'release: %s\n' "$1"; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }
version() {
    case "$1" in (*[!0-9.]*|""|.*|*.|*..*) usage;; esac
    case "$1" in ?*.?*.?*) ;; (*) usage;; esac
}
# The version a ref's package.json carries.
version_at() { git -C "$ROOT" show "$1:package.json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])'; }
tag_exists() { git -C "$ROOT" ls-remote --exit-code --tags origin "refs/tags/v$1" >/dev/null 2>&1; }

# Step 1: the release branch and PR. Sets PR and HEAD_SHA.
open_pr() {
    # The commit hooks run prettier from ./node_modules; the scratch worktree
    # borrows this checkout's.
    [ -d "$ROOT/node_modules" ] || die "no node_modules in $ROOT; run npm ci first"
    [ -z "$NOTES_FILE" ] || NOTES_FILE=$(cd -- "$(dirname -- "$NOTES_FILE")" && pwd -P)/$(basename -- "$NOTES_FILE")
    [ -z "$NOTES_FILE" ] || [ -s "$NOTES_FILE" ] || die "notes file $NOTES_FILE is missing or empty"
    BRANCH="release-$VER"

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
    HEAD_SHA=$(git -C "$WT" rev-parse HEAD)
    if [ "$PUSH" = 0 ]; then
        KEEP=1
        say "committed for $BRANCH in $WT (not pushed; remove with git worktree remove)"
        git -C "$WT" log --oneline origin/main..
        return
    fi
    git -C "$WT" push --quiet origin "HEAD:refs/heads/$BRANCH"
    PR=$(gh pr list --head "$BRANCH" --state open --json url --jq '.[0].url // empty')
    if [ -n "$PR" ]; then
        say "updated $PR"
    else
        # shellcheck disable=SC2016 # the backticks are Markdown
        BODY=$(printf 'Bumps every manifest to %s and turns its CHANGELOG section into the release notes.\n\nOpened by `sh scripts/release.sh %s`, which merges this PR once the required builds pass, tags the merge commit, and lets the tag publish the release.\n\n## Release notes\n\n%s\n' \
            "$VER" "$VER" "$(meta notes "$VER")")
        PR=$(gh pr create --base main --head "$BRANCH" --title "release $VER" --body "$BODY")
        say "opened $PR"
    fi
}

# Step 2: wait for the PR's required builds, then merge exactly that head.
merge_pr() {
    say "waiting for the required builds on $PR"
    i=0
    until [ "$(gh pr checks "$PR" --required --json name --jq length 2>/dev/null || echo 0)" -gt 0 ]; do
        i=$((i + 1)); [ $i -le 60 ] || die "no required checks started on $PR after 10 minutes"
        sleep 10
    done
    gh pr checks "$PR" --required --watch --fail-fast --interval 60 >/dev/null \
        || die "a required build failed on $PR; fix it on the branch and rerun"
    gh pr merge "$PR" --merge --match-head-commit "$HEAD_SHA"
    say "merged $PR"
}

# Step 3: tag the release PR's merge commit and push the tag.
push_tag() {
    SHA=$(gh pr list --head "release-$VER" --state merged --json mergeCommit --jq '.[0].mergeCommit.oid // empty')
    [ -n "$SHA" ] || die "origin/main is at $VER but no merged release-$VER PR was found; tag it by hand"
    git -C "$ROOT" fetch --quiet origin main
    git -C "$ROOT" merge-base --is-ancestor "$SHA" origin/main || die "merge commit $SHA is not on origin/main"
    [ "$(version_at "$SHA")" = "$VER" ] || die "merge commit $SHA is not at $VER"
    git -C "$ROOT" tag -f -a "v$VER" -m "v$VER" "$SHA"
    git -C "$ROOT" push --quiet origin "refs/tags/v$VER"
    say "pushed v$VER at $SHA"
}

# Step 4: the tag's release run builds and publishes; follow it.
watch_publish() {
    RUN=""; i=0
    while [ -z "$RUN" ]; do
        i=$((i + 1)); [ $i -le 30 ] || die "no release run for v$VER appeared; check the Actions tab"
        sleep 5
        RUN=$(gh run list --workflow release.yml --event push --limit 20 --json databaseId,headBranch \
            --jq "[.[] | select(.headBranch == \"v$VER\")][0].databaseId // empty")
    done
    say "building and publishing in run $RUN"
    gh run watch "$RUN" --exit-status --interval 60 >/dev/null \
        || die "release run $RUN failed: gh run view $RUN --log-failed (rerun it from the Actions tab)"
    say "published $(gh release view "v$VER" --json url --jq .url)"
}

cmd_release() {
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
    git -C "$ROOT" fetch --quiet origin main
    # Resume from wherever an earlier run stopped.
    if tag_exists "$VER"; then
        gh release view "v$VER" --json isDraft --jq .isDraft 2>/dev/null | grep -qx false \
            && die "v$VER is already published"
        watch_publish
        return
    fi
    if [ "$(version_at origin/main)" != "$VER" ]; then
        open_pr
        [ "$PUSH" = 1 ] || return 0
        merge_pr
    fi
    push_tag
    watch_publish
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
    say "packages in $OUT; smoke them with python3 e2e/package-smoke.py $OUT"
}

case "${1:-}" in
    build) shift; cmd_build "$@" ;;
    "" | -*) usage ;;
    *) cmd_release "$@" ;;
esac
