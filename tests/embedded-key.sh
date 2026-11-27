#!/bin/sh
set -eu

ROOT=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d "${TMPDIR:-/tmp}/ark-embedded-key-test.XXXXXX")
SERVER_PID=

cleanup() {
    [ -z "$SERVER_PID" ] || kill "$SERVER_PID" 2> /dev/null || true
    rm -rf "$TMP"
}
trap cleanup 0 1 2 15

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

unset ARK_UPDATE_PUBLIC_KEY
openssl genpkey -algorithm ED25519 -out "$TMP/private.pem" > /dev/null 2>&1
openssl pkey -in "$TMP/private.pem" -pubout -out "$TMP/public.pem" > /dev/null 2>&1
cd "$ROOT"
ARK_UPDATE_PUBLIC_KEY_PEM="$(cat "$TMP/public.pem")" \
    cargo build --bin ark-updater --features updater > "$TMP/build.out" 2>&1

HOME_DIR="$TMP/home"
DATA="$HOME_DIR/data"
STATE="$HOME_DIR/state"
mkdir -p "$DATA/ark/versions/0.1.0-beta" "$STATE/ark" "$TMP/dist"
printf 'fixture\n' > "$DATA/ark/versions/0.1.0-beta/ark"
PORT=$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(('127.0.0.1', 0))
print(s.getsockname()[1])
s.close()
PY
)
cat > "$STATE/ark/install.json" <<EOF
{
  "version": "0.1.0-beta",
  "arch": "x86_64",
  "channel": "stable",
  "manifest_url": "http://127.0.0.1:$PORT/latest.json",
  "installed_at": "2026-11-27T00:00:00Z"
}
EOF
cat > "$TMP/dist/latest.json" <<EOF
{
  "version": "0.1.1",
  "channel": "stable",
  "published_at": "2026-11-27T00:00:00Z",
  "minimum_version": "0.1.0-beta",
  "critical": false,
  "notes_url": "https://example.com/releases/0.1.1",
  "assets": {
    "x86_64-unknown-linux-gnu": {
      "url": "http://127.0.0.1:$PORT/ark.tar.gz",
      "size": 1,
      "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
      "signature": "http://127.0.0.1:$PORT/ark.tar.gz.sig"
    }
  }
}
EOF
openssl pkeyutl -sign -inkey "$TMP/private.pem" -rawin -in "$TMP/dist/latest.json" -out "$TMP/dist/latest.json.sig"
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$TMP/dist" > "$TMP/server.log" 2>&1 &
SERVER_PID=$!
i=0
while ! curl -fsS "http://127.0.0.1:$PORT/latest.json" > /dev/null 2>&1; do
    i=$((i + 1))
    [ "$i" -lt 100 ] || fail "server did not start"
    sleep 0.05
done

env \
    HOME="$HOME_DIR" \
    XDG_DATA_HOME="$DATA" \
    XDG_BIN_HOME="$HOME_DIR/bin" \
    XDG_STATE_HOME="$STATE" \
    XDG_CACHE_HOME="$HOME_DIR/cache" \
    XDG_CONFIG_HOME="$HOME_DIR/config" \
    ARK_ALLOW_INSECURE_HTTP=1 \
    ARK_DOWNLOAD_TOOL=curl \
    "$ROOT/target/debug/ark-updater" check > "$TMP/check.out" 2>&1

grep -F 'event: update-available' "$TMP/check.out" > /dev/null || fail "embedded key did not verify manifest"
echo "embedded update key tests passed"
