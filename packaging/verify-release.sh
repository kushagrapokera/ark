#!/bin/sh
# Verify a release archive's checksum, layout, paths, executables, and optional signatures.
set -eu
umask 022

if [ "$#" -lt 1 ] || [ "$#" -gt 3 ]; then
    echo "usage: $0 <archive.tar.gz> [SHA256SUMS] [public-key.pem]" >&2
    exit 2
fi

ARCHIVE=$1
CHECKSUM_FILE=${2:-$(dirname "$ARCHIVE")/SHA256SUMS}
PUBLIC_KEY=${3:-}
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
ARCHIVE=$(CDPATH='' cd "$(dirname "$ARCHIVE")" && pwd)/$(basename "$ARCHIVE")
CHECKSUM_FILE=$(CDPATH='' cd "$(dirname "$CHECKSUM_FILE")" && pwd)/$(basename "$CHECKSUM_FILE")

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
[ -x "$RELEASE_DIR/ark-updater" ] || die "release ark-updater executable missing"
[ -f "$RELEASE_DIR/ark.desktop" ] || die "release desktop entry missing"
[ -f "$RELEASE_DIR/icons/ark.png" ] || die "release icon missing"
[ -f "$RELEASE_DIR/install-release.sh" ] || die "release installer missing"
[ -f "$RELEASE_DIR/uninstall.sh" ] || die "release uninstaller missing"

VERSION_OUTPUT=$("$RELEASE_DIR/ark" --version 2> /dev/null) || die "release ark failed --version"
case "$VERSION_OUTPUT" in
    ark\ *) ;;
    *) die "unexpected --version output: $VERSION_OUTPUT" ;;
esac
UPDATER_VERSION_OUTPUT=$("$RELEASE_DIR/ark-updater" --version 2> /dev/null) || die "release ark-updater failed --version"
case "$UPDATER_VERSION_OUTPUT" in
    ark-updater\ *) ;;
    *) die "unexpected ark-updater --version output: $UPDATER_VERSION_OUTPUT" ;;
esac

if [ -n "$PUBLIC_KEY" ]; then
    need openssl
    need python3
    [ -f "$PUBLIC_KEY" ] || die "public signing key not found: $PUBLIC_KEY"
    DIST=$(CDPATH='' cd "$(dirname "$ARCHIVE")" && pwd)
    INSTALLER="$DIST/ark-installer.sh"
    MANIFEST="$DIST/latest.json"
    CHECKSUM_SIGNATURE="$CHECKSUM_FILE.sig"
    for required in \
        "$ARCHIVE.sig" \
        "$INSTALLER" \
        "$INSTALLER.sig" \
        "$CHECKSUM_SIGNATURE" \
        "$MANIFEST" \
        "$MANIFEST.sig"; do
        [ -f "$required" ] || die "required signed release asset missing: $required"
    done

    verify_signature() {
        file=$1
        signature=$2
        openssl pkeyutl -verify -pubin -rawin -inkey "$PUBLIC_KEY" -in "$file" -sigfile "$signature" > /dev/null 2>&1 \
            || die "signature verification failed for $(basename "$file")"
    }
    verify_signature "$ARCHIVE" "$ARCHIVE.sig"
    verify_signature "$INSTALLER" "$INSTALLER.sig"
    verify_signature "$MANIFEST" "$MANIFEST.sig"
    verify_signature "$CHECKSUM_FILE" "$CHECKSUM_SIGNATURE"

    for asset in \
        "$BASE" \
        "$BASE.sig" \
        ark-installer.sh \
        ark-installer.sh.sig \
        latest.json \
        latest.json.sig; do
        entry=$(awk -v name="$asset" '$2 == name || $2 == "*" name { print $1; exit }' "$CHECKSUM_FILE")
        [ -n "$entry" ] || die "no checksum entry for $asset"
    done
    (
        cd "$DIST"
        sha256sum -c "$CHECKSUM_FILE" > /dev/null
    ) || die "one or more release checksums failed"

    VERSION=${BASE#ark-}
    VERSION=${VERSION%-linux-x86_64.tar.gz}
    [ "$VERSION_OUTPUT" = "ark $VERSION" ] || die "ark version does not match archive name"
    [ "$UPDATER_VERSION_OUTPUT" = "ark-updater $VERSION" ] || die "ark-updater version does not match archive name"
    python3 - "$MANIFEST" "$BASE" "$VERSION" "$ACTUAL" "$(wc -c < "$ARCHIVE" | tr -d ' ')" <<'PY' || die "latest.json does not match the signed archive"
import json
import sys

manifest_path, archive_name, version, checksum, size = sys.argv[1:]

def require(condition, message):
    if not condition:
        raise SystemExit(message)

with open(manifest_path, encoding="utf-8") as handle:
    manifest = json.load(handle)
require(manifest["version"] == version, "manifest version mismatch")
require(manifest["channel"] == "stable", "manifest channel must be stable")
asset = manifest["assets"]["x86_64-unknown-linux-gnu"]
require(asset["url"].startswith("https://"), "asset URL must use HTTPS")
require(asset["url"].endswith("/" + archive_name), "asset URL does not match archive")
require(asset["signature"].startswith("https://"), "signature URL must use HTTPS")
require(asset["signature"].endswith("/" + archive_name + ".sig"), "signature URL does not match archive")
require(int(asset["size"]) == int(size), "manifest size mismatch")
require(asset["sha256"] == checksum, "manifest checksum mismatch")
require(manifest["notes_url"].startswith("https://"), "notes URL must use HTTPS")
PY
fi

printf 'Verified: %s (%s)\n' "$BASE" "$VERSION_OUTPUT"
if [ -n "$PUBLIC_KEY" ]; then
    printf 'Signed release verified: %s\n' "$BASE"
fi
