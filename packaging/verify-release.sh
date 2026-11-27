#!/bin/sh
# Verify a release archive's checksum, layout, paths, and executable.
set -eu
umask 022

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "usage: $0 <archive.tar.gz> [SHA256SUMS]" >&2
    exit 2
fi

ARCHIVE=$1
CHECKSUM_FILE=${2:-$(dirname "$ARCHIVE")/SHA256SUMS}
TMP=$(mktemp -d "${TMPDIR:-/tmp}/ark-release-verify.XXXXXX")
trap 'rm -rf "$TMP"' 0 1 2 15

die() {
    echo "ark verify: $*" >&2
    exit 1
}

need() {
    command -v "$1" > /dev/null 2>&1 || die "required command not found: $1"
}

need sha256sum
need tar
need awk
need grep
need mktemp

[ -f "$ARCHIVE" ] || die "archive not found: $ARCHIVE"
[ -f "$CHECKSUM_FILE" ] || die "checksum file not found: $CHECKSUM_FILE"

BASE=$(basename "$ARCHIVE")
EXPECTED=$(awk -v name="$BASE" '$2 == name || $2 == "*" name { print $1; exit }' "$CHECKSUM_FILE")
[ -n "$EXPECTED" ] || die "no checksum entry for $BASE"
ACTUAL=$(sha256sum "$ARCHIVE" | awk '{print $1}')
[ "$ACTUAL" = "$EXPECTED" ] || die "checksum mismatch for $BASE"

tar -tzf "$ARCHIVE" > "$TMP/list" || die "archive is not a readable tar.gz"
if grep -E '(^/|(^|/)\.\.(/|$))' "$TMP/list" > /dev/null; then
    die "archive contains an absolute or parent-traversal path"
fi

tar -tvzf "$ARCHIVE" > "$TMP/verbose" || die "could not inspect archive entries"
if grep -E '^[lh]' "$TMP/verbose" > /dev/null; then
    die "archive contains symlinks or hard links"
fi

mkdir -p "$TMP/extract"
tar -xzf "$ARCHIVE" -C "$TMP/extract"
set -- "$TMP/extract"/*
[ "$#" -eq 1 ] || die "archive must contain exactly one top-level directory"
RELEASE_DIR=$1
[ -d "$RELEASE_DIR" ] || die "release directory missing"
[ -x "$RELEASE_DIR/ark" ] || die "release ark executable missing"
[ -f "$RELEASE_DIR/ark.desktop" ] || die "release desktop entry missing"
[ -f "$RELEASE_DIR/icons/ark.png" ] || die "release icon missing"
[ -f "$RELEASE_DIR/install-release.sh" ] || die "release installer missing"
[ -f "$RELEASE_DIR/uninstall.sh" ] || die "release uninstaller missing"

VERSION_OUTPUT=$("$RELEASE_DIR/ark" --version 2> /dev/null) || die "release ark failed --version"
case "$VERSION_OUTPUT" in
    ark\ *) ;;
    *) die "unexpected --version output: $VERSION_OUTPUT" ;;
esac

printf 'Verified: %s (%s)\n' "$BASE" "$VERSION_OUTPUT"
