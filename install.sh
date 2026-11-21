#!/usr/bin/env bash
# Install ark: build the release binary, register `ark` on PATH and create
# the desktop entry. Run from the repo root:  ./install.sh
# Requires sudo (for /usr/local/bin) — you will be prompted once.
set -euo pipefail
cd "$(dirname "$0")"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found — install rustup first: https://rustup.rs" >&2
  exit 1
fi

cargo build --release
BIN="$(pwd)/target/release/ark"

echo "Installing $BIN -> /usr/local/bin/ark"
sudo install -m 755 "$BIN" /usr/local/bin/ark

DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$DESKTOP_DIR"
cat > "$DESKTOP_DIR/ark.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=ark
GenericName=Code Editor
Comment=Ultra-lightweight agent-first code editor
Exec=/usr/local/bin/ark %f
Terminal=false
Categories=Development;IDE;TextEditor;
StartupNotify=true
EOF

echo "Installed desktop entry: $DESKTOP_DIR/ark.desktop"
echo "Done. Run 'ark .' in any directory."
