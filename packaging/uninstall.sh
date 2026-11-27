#!/bin/sh
# Uninstall a user-local ark release. Project files are never touched.
set -eu
umask 022

PURGE=0
case "${1-}" in
    '') ;;
    --purge) PURGE=1 ;;
    *) echo "usage: $0 [--purge]" >&2; exit 2 ;;
esac

DATA_HOME=${XDG_DATA_HOME:-$HOME/.local/share}
BIN_HOME=${XDG_BIN_HOME:-$HOME/.local/bin}
CONFIG_HOME=${XDG_CONFIG_HOME:-$HOME/.config}
STATE_HOME=${XDG_STATE_HOME:-$HOME/.local/state}
CACHE_HOME=${XDG_CACHE_HOME:-$HOME/.cache}
APP_DATA="$DATA_HOME/ark"
BIN_LINK="$BIN_HOME/ark"
DESKTOP_FILE="$DATA_HOME/applications/ark.desktop"
ICON_FILE="$DATA_HOME/icons/hicolor/256x256/apps/ark.png"

if [ -L "$BIN_LINK" ]; then
    TARGET=$(readlink "$BIN_LINK" || true)
    case "$TARGET" in
        *"$APP_DATA/current/ark") rm -f "$BIN_LINK" ;;
        *) echo "ark uninstall: leaving unrelated symlink: $BIN_LINK -> $TARGET" >&2 ;;
    esac
elif [ -e "$BIN_LINK" ]; then
    echo "ark uninstall: leaving non-symlink executable: $BIN_LINK" >&2
fi

if [ -f "$DESKTOP_FILE" ] && grep -q '^Name=ark$' "$DESKTOP_FILE"; then
    rm -f "$DESKTOP_FILE"
fi
rm -f "$ICON_FILE"
rm -rf "$APP_DATA"

if [ "$PURGE" -eq 1 ]; then
    rm -rf "$STATE_HOME/ark" "$CACHE_HOME/ark" "$CONFIG_HOME/ark"
    echo "ark uninstall: removed user state, cache, and config"
fi

if command -v update-desktop-database > /dev/null 2>&1; then
    update-desktop-database "$(dirname "$DESKTOP_FILE")" > /dev/null 2>&1 || true
fi

echo "ark uninstalled."
