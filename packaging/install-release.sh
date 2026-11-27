#!/bin/sh
# Install a prebuilt ark release into a versioned user-local directory.
set -eu
umask 022

SCRIPT_DIR=$(CDPATH= cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR"

STAGE=
BACKUP=
CURRENT_TMP=
BIN_TMP=
DESKTOP_TMP=
ICON_TMP=
STATE_TMP=

cleanup() {
    [ -z "$STAGE" ] || rm -rf "$STAGE"
    [ -z "$BACKUP" ] || rm -rf "$BACKUP"
    [ -z "$CURRENT_TMP" ] || rm -f "$CURRENT_TMP"
    [ -z "$BIN_TMP" ] || rm -f "$BIN_TMP"
    [ -z "$DESKTOP_TMP" ] || rm -f "$DESKTOP_TMP"
    [ -z "$ICON_TMP" ] || rm -f "$ICON_TMP"
    [ -z "$STATE_TMP" ] || rm -f "$STATE_TMP"
}
trap cleanup 0 1 2 15

die() {
    echo "ark install: $*" >&2
    exit 1
}

warn() {
    echo "ark install: warning: $*" >&2
}

need() {
    command -v "$1" > /dev/null 2>&1 || die "required command not found: $1"
}

need uname
need mkdir
need cp
need mv
need ln
need rm
need chmod
need sed
need head
need dirname
need cat
need date
need readlink

OS=$(uname -s)
[ "$OS" = "Linux" ] || die "unsupported operating system: $OS"

ARCH=${ARK_ARCH:-$(uname -m)}
case "$ARCH" in
    x86_64) ;;
    aarch64 | arm64) ARCH=aarch64 ;;
    *) die "unsupported architecture: $ARCH" ;;
esac

[ -x "./ark" ] || die "bundled ark executable is missing"
VERSION_OUTPUT=$(./ark --version 2> /dev/null) || die "bundled ark failed --version"
VERSION=$(printf '%s\n' "$VERSION_OUTPUT" | sed -n 's/^ark //p' | head -n 1)
case "$VERSION" in
    '' | *[!A-Za-z0-9.+-]*) die "invalid bundled ark version: $VERSION" ;;
esac

DATA_HOME=${XDG_DATA_HOME:-$HOME/.local/share}
BIN_HOME=${XDG_BIN_HOME:-$HOME/.local/bin}
STATE_HOME=${XDG_STATE_HOME:-$HOME/.local/state}
APP_DATA="$DATA_HOME/ark"
VERSIONS_DIR="$APP_DATA/versions"
VERSION_DIR="$VERSIONS_DIR/$VERSION"
CURRENT_LINK="$APP_DATA/current"
BIN_LINK="$BIN_HOME/ark"
DESKTOP_FILE="$DATA_HOME/applications/ark.desktop"
ICON_FILE="$DATA_HOME/icons/hicolor/256x256/apps/ark.png"
STATE_FILE="$STATE_HOME/ark/install.json"
MANIFEST_URL=${ARK_MANIFEST_URL:-https://github.com/kushagrapokera/ark/releases/latest/download/latest.json}
CHANNEL=${ARK_CHANNEL:-stable}

case "$MANIFEST_URL" in
    https://*) ;;
    *) die "ARK_MANIFEST_URL must use HTTPS" ;;
esac
case "$MANIFEST_URL" in
    *[\"\\]*) die "ARK_MANIFEST_URL contains invalid JSON characters" ;;
esac
case "$CHANNEL" in
    '' | *[!A-Za-z0-9.+-]*) die "invalid update channel: $CHANNEL" ;;
esac

mkdir -p "$APP_DATA" "$VERSIONS_DIR" "$BIN_HOME" \
    "$(dirname "$DESKTOP_FILE")" "$(dirname "$ICON_FILE")" "$(dirname "$STATE_FILE")"

STAGE="$VERSIONS_DIR/.$VERSION.tmp.$$"
BACKUP="$VERSIONS_DIR/.$VERSION.old.$$"
rm -rf "$STAGE" "$BACKUP"
mkdir -p "$STAGE"
cp ./ark "$STAGE/ark"
chmod 755 "$STAGE/ark"
if [ -f ./ark-updater ]; then
    cp ./ark-updater "$STAGE/ark-updater"
    chmod 755 "$STAGE/ark-updater"
fi
for doc in README.md LICENSE CHANGELOG.md; do
    if [ -f "./$doc" ]; then
        cp "./$doc" "$STAGE/$doc"
    fi
done

"$STAGE/ark" --version > /dev/null 2>&1 || die "staged ark failed --version"

if [ -e "$VERSION_DIR" ] || [ -L "$VERSION_DIR" ]; then
    mv "$VERSION_DIR" "$BACKUP"
fi
if ! mv "$STAGE" "$VERSION_DIR"; then
    if [ -e "$BACKUP" ] || [ -L "$BACKUP" ]; then
        mv "$BACKUP" "$VERSION_DIR"
    fi
    die "could not install version $VERSION"
fi
STAGE=
rm -rf "$BACKUP"
BACKUP=

CURRENT_TMP="$APP_DATA/.current.tmp.$$"
ln -s "versions/$VERSION" "$CURRENT_TMP"
mv -Tf "$CURRENT_TMP" "$CURRENT_LINK"
CURRENT_TMP=

BIN_TMP="$BIN_HOME/.ark.tmp.$$"
ln -s "$CURRENT_LINK/ark" "$BIN_TMP"
mv -Tf "$BIN_TMP" "$BIN_LINK"
BIN_TMP=

if [ -f ./ark.desktop ]; then
    DESKTOP_TMP="$DESKTOP_FILE.tmp.$$"
    while IFS= read -r line || [ -n "$line" ]; do
        case "$line" in
            Exec=*) printf 'Exec=%s %%f\n' "$BIN_LINK" ;;
            *) printf '%s\n' "$line" ;;
        esac
    done < ./ark.desktop > "$DESKTOP_TMP"
    mv -f "$DESKTOP_TMP" "$DESKTOP_FILE"
    DESKTOP_TMP=
else
    warn "ark.desktop is missing; desktop integration was not installed"
fi

if [ -f ./icons/ark.png ]; then
    ICON_TMP="$ICON_FILE.tmp.$$"
    cp ./icons/ark.png "$ICON_TMP"
    mv -f "$ICON_TMP" "$ICON_FILE"
    ICON_TMP=
else
    warn "icons/ark.png is missing; the default icon will be used"
fi

INSTALLED_AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)
STATE_TMP="$STATE_FILE.tmp.$$"
cat > "$STATE_TMP" <<EOF
{
  "version": "$VERSION",
  "arch": "$ARCH",
  "channel": "$CHANNEL",
  "manifest_url": "$MANIFEST_URL",
  "installed_at": "$INSTALLED_AT"
}
EOF
mv -f "$STATE_TMP" "$STATE_FILE"
STATE_TMP=

if command -v desktop-file-validate > /dev/null 2>&1; then
    desktop-file-validate "$DESKTOP_FILE" || warn "desktop entry validation failed"
fi
if command -v update-desktop-database > /dev/null 2>&1; then
    update-desktop-database "$(dirname "$DESKTOP_FILE")" || warn "desktop database update failed"
fi

printf 'ark %s installed successfully.\n' "$VERSION"
printf 'Installed: %s\n' "$VERSION_DIR"
case ":$PATH:" in
    *":$BIN_HOME:"*) ;;
    *) printf 'Add this directory to PATH: %s\n' "$BIN_HOME" ;;
esac
printf 'Run: ark .\n'
