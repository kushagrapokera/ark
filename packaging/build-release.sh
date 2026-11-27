#!/bin/sh
# Build a release tarball containing only files users need.
set -eu
umask 022

ROOT=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

die() {
    echo "ark release: $*" >&2
    exit 1
}

need() {
    command -v "$1" > /dev/null 2>&1 || die "required command not found: $1"
}

need uname
need mkdir
need cp
need rm
need tar
need sha256sum
need awk
need sed

OS=$(uname -s)
[ "$OS" = "Linux" ] || die "unsupported operating system: $OS"
ARCH=${ARK_ARCH:-$(uname -m)}
case "$ARCH" in
    x86_64) ;;
    aarch64 | arm64) ARCH=aarch64 ;;
    *) die "unsupported architecture: $ARCH" ;;
esac

if [ -n "${ARK_VERSION:-}" ]; then
    VERSION=$ARK_VERSION
else
    need cargo
    VERSION=$(cargo pkgid | sed 's/.*#//')
fi
case "$VERSION" in
    '' | *[!A-Za-z0-9.+-]*) die "invalid release version: $VERSION" ;;
esac
REPO=${ARK_REPO:-kushagrapokera/ark}
case "$REPO" in
    '' | *[!A-Za-z0-9._/-]*) die "invalid release repository: $REPO" ;;
esac

if [ "${ARK_SKIP_BUILD:-0}" != "1" ]; then
    need cargo
    cargo build --release --locked --bin ark
    cargo build --release --locked --bin ark-updater --features updater
fi

BINARY=${ARK_BINARY:-$ROOT/target/release/ark}
ICON=${ARK_ICON:-$ROOT/assets/icons/ark.png}
UPDATER=${ARK_UPDATER:-$ROOT/target/release/ark-updater}
[ -x "$BINARY" ] || die "release binary not found: $BINARY (set ARK_BINARY to override)"
[ -x "$UPDATER" ] || die "release updater not found: $UPDATER (set ARK_UPDATER to override)"
[ -f "$ICON" ] || die "release icon not found: $ICON (set ARK_ICON to override)"

OUTPUT_DIR=${ARK_OUTPUT_DIR:-$ROOT/target/release}
mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR=$(CDPATH='' cd "$OUTPUT_DIR" && pwd)
RELEASE_NAME="ark-$VERSION-linux-$ARCH"
RELEASE_DIR="$OUTPUT_DIR/$RELEASE_NAME"
ARCHIVE="$OUTPUT_DIR/$RELEASE_NAME.tar.gz"

rm -rf "$RELEASE_DIR"
rm -f "$ARCHIVE" "$OUTPUT_DIR/SHA256SUMS"
mkdir -p "$RELEASE_DIR/icons"

cp "$BINARY" "$RELEASE_DIR/ark"
cp "$UPDATER" "$RELEASE_DIR/ark-updater"
chmod 755 "$RELEASE_DIR/ark" "$RELEASE_DIR/ark-updater"
cp "$ROOT/packaging/ark.desktop" "$RELEASE_DIR/ark.desktop"
cp "$ROOT/packaging/install-release.sh" "$RELEASE_DIR/install-release.sh"
cp "$ROOT/packaging/uninstall.sh" "$RELEASE_DIR/uninstall.sh"
cp "$ICON" "$RELEASE_DIR/icons/ark.png"
for doc in README.md LICENSE CHANGELOG.md; do
    cp "$ROOT/$doc" "$RELEASE_DIR/$doc"
done

tar -czf "$ARCHIVE" -C "$OUTPUT_DIR" "$RELEASE_NAME"
ARCHIVE_SHA=$(sha256sum "$ARCHIVE" | awk '{print $1}')
INSTALLER="$OUTPUT_DIR/ark-installer.sh"
sed \
    -e "s|@ARK_REPO@|$REPO|g" \
    -e "s|@ARK_VERSION@|$VERSION|g" \
    -e "s|@ARK_ARCH@|$ARCH|g" \
    -e "s|@ARK_SHA256@|$ARCHIVE_SHA|g" \
    "$ROOT/packaging/ark-installer.sh.in" > "$INSTALLER"
chmod 755 "$INSTALLER"
(
    cd "$OUTPUT_DIR"
    sha256sum "$RELEASE_NAME.tar.gz" ark-installer.sh > SHA256SUMS
)

printf 'Built: %s\n' "$ARCHIVE"
printf 'Installer: %s\n' "$INSTALLER"
printf 'Checksums: %s\n' "$OUTPUT_DIR/SHA256SUMS"
