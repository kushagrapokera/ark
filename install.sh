#!/usr/bin/env bash
# Install ark: build the release binary, register `ark` on PATH and create
# the desktop entry and icon.
#
#   ./install.sh          # system install at /usr/local/bin (uses sudo)
#   ./install.sh --user   # user install at ~/.local/bin (no sudo)
#
# Test an existing build without Cargo:
#
#   ARK_SKIP_BUILD=1 ./install.sh --user
#
# ARK_BINARY and ARK_ICON can override the source-tree paths for local tests.
# Run from the repo root. KDE ships its archiver as /usr/bin/ark; the
# installed directory must come earlier on PATH to shadow it (both
# /usr/local/bin and ~/.local/bin normally do).
set -euo pipefail
cd "$(dirname "$0")"

MODE="${1:-system}"
case "$MODE" in
  system|--system) BIN_DIR="/usr/local/bin"; SUDO=(sudo) ;;
  --user|user)     BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"; SUDO=() ;;
  *)
    echo "usage: $0 [--user]" >&2
    exit 2
    ;;
esac

ICON="${ARK_ICON:-$(pwd)/assets/icons/ark.png}"
if [[ ! -f "$ICON" ]]; then
  echo "desktop icon not found: $ICON (set ARK_ICON to override)" >&2
  exit 1
fi

if [[ "${ARK_SKIP_BUILD:-0}" != "1" ]]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found — install rustup first: https://rustup.rs" >&2
    exit 1
  fi
  cargo build --release --locked
fi

BIN="${ARK_BINARY:-$(pwd)/target/release/ark}"
if [[ ! -x "$BIN" ]]; then
  echo "release binary not found: $BIN (build first or set ARK_BINARY)" >&2
  exit 1
fi

mkdir -p "$BIN_DIR"
echo "Installing $BIN -> $BIN_DIR/ark"
if [[ ${#SUDO[@]} -eq 0 ]]; then
  install -m 755 "$BIN" "$BIN_DIR/ark"
else
  sudo install -m 755 "$BIN" "$BIN_DIR/ark"
fi
if ! INSTALLED_VERSION="$("$BIN_DIR/ark" --version)"; then
  echo "installed ark failed to run --version" >&2
  exit 1
fi

if [[ -e /usr/bin/ark && "$BIN_DIR" == "$HOME/.local/bin" ]]; then
  echo "Note: KDE Ark exists at /usr/bin/ark; $BIN_DIR/ark shadows it when"
  echo "      $BIN_DIR is earlier on PATH (it normally is)."
fi

DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/256x256/apps"
mkdir -p "$DESKTOP_DIR" "$ICON_DIR"
install -m 644 "$ICON" "$ICON_DIR/ark.png"
cat > "$DESKTOP_DIR/ark.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=ark
GenericName=Code Editor
Comment=Ultra-lightweight agent-first code editor
Exec="$BIN_DIR/ark" %f
Icon=ark
Terminal=false
Categories=Development;IDE;TextEditor;
StartupNotify=true
StartupWMClass=ark
EOF

echo "Installed desktop entry: $DESKTOP_DIR/ark.desktop"
echo "Installed desktop icon: $ICON_DIR/ark.png"
echo "Installed version: $INSTALLED_VERSION"
echo "Done. Run 'ark .' in any directory (use 'hash -r' in an existing shell)."
