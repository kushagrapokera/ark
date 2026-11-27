#!/bin/sh
# Sign a built ark release with Ed25519 and generate latest.json.
set -eu
umask 077

if [ "$#" -lt 4 ] || [ "$#" -gt 5 ]; then
    echo "usage: $0 <dist-dir> <version> <private-key> <minimum-version> [repository]" >&2
    exit 2
fi

DIST=$1
VERSION=$2
PRIVATE_KEY=$3
MINIMUM_VERSION=$4
REPO=${5:-kushagrapokera/ark}
PUBLIC_TMP=

cleanup() {
    [ -z "$PUBLIC_TMP" ] || rm -f "$PUBLIC_TMP"
}
trap cleanup 0 1 2 15

die() {
    echo "ark sign: $*" >&2
    exit 1
}

need() {
    command -v "$1" > /dev/null 2>&1 || die "required command not found: $1"
}

need openssl
need sha256sum
need python3
need date

case "$VERSION" in
    '' | *[!A-Za-z0-9.+-]*) die "invalid release version: $VERSION" ;;
esac
case "$MINIMUM_VERSION" in
    '' | *[!A-Za-z0-9.+-]*) die "invalid minimum version: $MINIMUM_VERSION" ;;
esac
case "$REPO" in
    '' | *[!A-Za-z0-9._/-]*) die "invalid repository: $REPO" ;;
esac

[ -d "$DIST" ] || die "release directory not found: $DIST"
DIST=$(CDPATH='' cd "$DIST" && pwd)
[ -f "$PRIVATE_KEY" ] || die "private signing key not found: $PRIVATE_KEY"
PRIVATE_KEY=$(CDPATH='' cd "$(dirname "$PRIVATE_KEY")" && pwd)/$(basename "$PRIVATE_KEY")
case "$PRIVATE_KEY" in
    "$DIST"/*) die "private signing key must not be inside the release directory" ;;
esac

ARCHIVE="$DIST/ark-$VERSION-linux-x86_64.tar.gz"
INSTALLER="$DIST/ark-installer.sh"
MANIFEST="$DIST/latest.json"
CHECKSUMS="$DIST/SHA256SUMS"
[ -f "$ARCHIVE" ] || die "release archive not found: $ARCHIVE"
[ -f "$INSTALLER" ] || die "installer not found: $INSTALLER"

PUBLIC_TMP=$(mktemp "${TMPDIR:-/tmp}/ark-release-public.XXXXXX")
openssl pkey -in "$PRIVATE_KEY" -pubout -out "$PUBLIC_TMP" > /dev/null 2>&1 \
    || die "could not derive public key from private signing key"

sign_file() {
    file=$1
    output=$2
    openssl pkeyutl -sign -inkey "$PRIVATE_KEY" -rawin -in "$file" -out "$output" \
        || die "could not sign $file"
}

verify_file() {
    file=$1
    signature=$2
    openssl pkeyutl -verify -pubin -rawin -inkey "$PUBLIC_TMP" -in "$file" -sigfile "$signature" > /dev/null 2>&1 \
        || die "signature verification failed for $file"
}

sign_file "$ARCHIVE" "$ARCHIVE.sig"
sign_file "$INSTALLER" "$INSTALLER.sig"

PUBLISHED_AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)
python3 - "$ARCHIVE" "$VERSION" "$MINIMUM_VERSION" "$REPO" "$PUBLISHED_AT" > "$MANIFEST" <<'PY'
import hashlib
import json
import pathlib
import sys

archive = pathlib.Path(sys.argv[1])
version, minimum, repo, published = sys.argv[2:]
base = f"https://github.com/{repo}/releases"
asset_base = f"{base}/download/v{version}"
manifest = {
    "version": version,
    "channel": "stable",
    "published_at": published,
    "minimum_version": minimum,
    "critical": False,
    "notes_url": f"{base}/tag/v{version}",
    "assets": {
        "x86_64-unknown-linux-gnu": {
            "url": f"{asset_base}/{archive.name}",
            "size": archive.stat().st_size,
            "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
            "signature": f"{asset_base}/{archive.name}.sig",
        }
    },
}
json.dump(manifest, sys.stdout, indent=2)
sys.stdout.write("\n")
PY
sign_file "$MANIFEST" "$MANIFEST.sig"

(
    cd "$DIST"
    sha256sum \
        "ark-$VERSION-linux-x86_64.tar.gz" \
        "ark-$VERSION-linux-x86_64.tar.gz.sig" \
        ark-installer.sh \
        ark-installer.sh.sig \
        latest.json \
        latest.json.sig > SHA256SUMS
)
sign_file "$CHECKSUMS" "$CHECKSUMS.sig"

verify_file "$ARCHIVE" "$ARCHIVE.sig"
verify_file "$INSTALLER" "$INSTALLER.sig"
verify_file "$MANIFEST" "$MANIFEST.sig"
verify_file "$CHECKSUMS" "$CHECKSUMS.sig"
(
    cd "$DIST"
    sha256sum -c SHA256SUMS > /dev/null
) || die "checksum verification failed"

printf 'Signed: %s\n' "$ARCHIVE"
printf 'Manifest: %s\n' "$MANIFEST"
printf 'Checksums: %s\n' "$CHECKSUMS"
