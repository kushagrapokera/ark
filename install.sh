#!/usr/bin/env bash
# Install ark: build the release binary, register `ark` on PATH and create
# the desktop entry.
#
#   ./install.sh          # system install at /usr/local/bin (uses sudo)
#   ./install.sh --user   # user install at ~/.local/bin (no sudo)
#
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

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found — install rustup first: https://rustup.rs" >&2
  exit 1
fi

cargo build --release
BIN="$(pwd)/target/release/ark"

mkdir -p "$BIN_DIR"
echo "Installing $BIN -> $BIN_DIR/ark"
if [[ ${#SUDO[@]} -eq 0 ]]; then
  install -m 755 "$BIN" "$BIN_DIR/ark"
else
  sudo install -m 755 "$BIN" "$BIN_DIR/ark"
fi

if [[ -e /usr/bin/ark && "$BIN_DIR" == "$HOME/.local/bin" ]]; then
  echo "Note: KDE Ark exists at /usr/bin/ark; $BIN_DIR/ark shadows it when"
  echo "      $BIN_DIR is earlier on PATH (it normally is)."
fi

DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$DESKTOP_DIR"
cat > "$DESKTOP_DIR/ark.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=ark
GenericName=Code Editor
Comment=Ultra-lightweight agent-first code editor
Exec=$BIN_DIR/ark %f
Terminal=false
Categories=Development;IDE;TextEditor;
StartupNotify=true
EOF

echo "Installed desktop entry: $DESKTOP_DIR/ark.desktop"
echo "Done. Run 'ark .' in any directory (use 'hash -r' in an existing shell)."
