#!/bin/sh
set -eu

ROOT=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d "${TMPDIR:-/tmp}/ark-update-smoke.XXXXXX")
SERVER_PID=
DOWNLOAD_PID=
APP_PID=

cleanup() {
    [ -z "$APP_PID" ] || kill "$APP_PID" 2> /dev/null || true
    [ -z "$DOWNLOAD_PID" ] || kill "$DOWNLOAD_PID" 2> /dev/null || true
    [ -z "$SERVER_PID" ] || kill "$SERVER_PID" 2> /dev/null || true
    if [ "${KEEP_TMP:-0}" != "1" ]; then
        rm -rf "$TMP"
    fi
}
trap cleanup 0 1 2 15

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

assert_eq() {
    [ "$1" = "$2" ] || fail "expected '$2', got '$1'"
}

make_binary() {
    path=$1
    version=$2
    cat > "$path" <<EOF
#!/bin/sh
if [ "\${1-}" = "--version" ]; then
    echo "ark $version"
    exit 0
fi
exit 0
EOF
    chmod 755 "$path"
}

start_server() {
    mode=$1
    if [ "$mode" = slow ]; then
        python3 "$TMP/slow-server.py" "$PORT" "$DIST" > "$TMP/server.log" 2>&1 &
    else
        python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$DIST" > "$TMP/server.log" 2>&1 &
    fi
    SERVER_PID=$!
    i=0
    while ! curl -fsS "http://127.0.0.1:$PORT/latest.json" > /dev/null 2>&1; do
        i=$((i + 1))
        [ "$i" -lt 100 ] || fail "update server did not start"
        sleep 0.05
    done
}

stop_server() {
    [ -z "$SERVER_PID" ] || kill "$SERVER_PID" 2> /dev/null || true
    SERVER_PID=
    i=0
    while [ "$i" -lt 100 ] && curl -fsS "http://127.0.0.1:$PORT/latest.json" > /dev/null 2>&1; do
        i=$((i + 1))
        sleep 0.02
    done
}

openssl genpkey -algorithm ED25519 -out "$TMP/private.pem" > /dev/null 2>&1
openssl pkey -in "$TMP/private.pem" -pubout -out "$TMP/public.pem" > /dev/null 2>&1
cd "$ROOT"
ARK_UPDATE_PUBLIC_KEY_PEM=$(cat "$TMP/public.pem")
export ARK_UPDATE_PUBLIC_KEY_PEM
cargo build --release --locked --bin ark > "$TMP/build-ark.out" 2>&1
cargo build --release --locked --bin ark-updater --features updater > "$TMP/build-updater.out" 2>&1

OLD_DIST="$TMP/old-dist"
ARK_VERSION=0.1.0-beta \
ARK_ARCH=x86_64 \
ARK_BINARY="$ROOT/target/release/ark" \
ARK_UPDATER="$ROOT/target/release/ark-updater" \
ARK_ICON="$ROOT/assets/icons/ark.png" \
ARK_SKIP_BUILD=1 \
ARK_OUTPUT_DIR="$OLD_DIST" \
    sh packaging/build-release.sh > "$TMP/build-old.out" 2>&1

make_binary "$TMP/ark-0.1.1" 0.1.1
DIST="$TMP/dist"
ARK_VERSION=0.1.1 \
ARK_ARCH=x86_64 \
ARK_REPO=example/ark \
ARK_BINARY="$TMP/ark-0.1.1" \
ARK_UPDATER="$ROOT/target/release/ark-updater" \
ARK_ICON="$ROOT/assets/icons/ark.png" \
ARK_SKIP_BUILD=1 \
ARK_OUTPUT_DIR="$DIST" \
    sh packaging/build-release.sh > "$TMP/build-new.out" 2>&1
ARCHIVE="$DIST/ark-0.1.1-linux-x86_64.tar.gz"
ARCHIVE_SIZE=$(wc -c < "$ARCHIVE" | tr -d ' ')
CHECKSUM=$(sha256sum "$ARCHIVE" | awk '{print $1}')
openssl pkeyutl -sign -inkey "$TMP/private.pem" -rawin -in "$ARCHIVE" -out "$ARCHIVE.sig"

PORT=$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(('127.0.0.1', 0))
print(s.getsockname()[1])
s.close()
PY
)
cat > "$DIST/latest.json" <<EOF
{
  "version": "0.1.1",
  "channel": "stable",
  "published_at": "2026-11-27T00:00:00Z",
  "minimum_version": "0.1.0-beta",
  "critical": false,
  "notes_url": "https://example.com/releases/0.1.1",
  "assets": {
    "x86_64-unknown-linux-gnu": {
      "url": "http://127.0.0.1:$PORT/ark-0.1.1-linux-x86_64.tar.gz",
      "size": $ARCHIVE_SIZE,
      "sha256": "$CHECKSUM",
      "signature": "http://127.0.0.1:$PORT/ark-0.1.1-linux-x86_64.tar.gz.sig"
    }
  }
}
EOF
openssl pkeyutl -sign -inkey "$TMP/private.pem" -rawin -in "$DIST/latest.json" -out "$DIST/latest.json.sig"
cp "$DIST/latest.json.sig" "$TMP/latest.json.sig.good"

HOME_DIR="$TMP/home-${TMP##*/}"
DATA="$HOME_DIR/data"
BIN="$HOME_DIR/bin"
STATE="$HOME_DIR/state"
mkdir -p "$HOME_DIR" "$TMP/old-extract"
tar -xzf "$OLD_DIST/ark-0.1.0-beta-linux-x86_64.tar.gz" -C "$TMP/old-extract"
env \
    HOME="$HOME_DIR" \
    XDG_DATA_HOME="$DATA" \
    XDG_BIN_HOME="$BIN" \
    XDG_STATE_HOME="$STATE" \
    XDG_CACHE_HOME="$HOME_DIR/cache" \
    XDG_CONFIG_HOME="$HOME_DIR/config" \
    sh "$TMP/old-extract/ark-0.1.0-beta-linux-x86_64/install-release.sh" > "$TMP/install.out" 2>&1
python3 - "$STATE/ark/install.json" "http://127.0.0.1:$PORT/latest.json" <<'PY'
import json
import pathlib
import sys
path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
data["manifest_url"] = sys.argv[2]
path.write_text(json.dumps(data, indent=2) + "\n")
PY
UPDATER="$DATA/ark/current/ark-updater"
printf 'project: unchanged\n' > "$HOME_DIR/project.yaml"
start_server normal

run_updater() {
    env \
        HOME="$HOME_DIR" \
        XDG_DATA_HOME="$DATA" \
        XDG_BIN_HOME="$BIN" \
        XDG_STATE_HOME="$STATE" \
        XDG_CACHE_HOME="$HOME_DIR/cache" \
        XDG_CONFIG_HOME="$HOME_DIR/config" \
        ARK_ALLOW_INSECURE_HTTP=1 \
        ARK_DOWNLOAD_TOOL=curl \
        "$UPDATER" "$@"
}

wait_for_log() {
    file=$1
    pattern=$2
    i=0
    while ! grep -F "$pattern" "$file" > /dev/null 2>&1; do
        i=$((i + 1))
        [ "$i" -lt 400 ] || fail "timed out waiting for $pattern"
        sleep 0.05
    done
}

ui_update() {
    env \
        HOME="$HOME_DIR" \
        XDG_DATA_HOME="$DATA" \
        XDG_BIN_HOME="$BIN" \
        XDG_STATE_HOME="$STATE" \
        XDG_CACHE_HOME="$HOME_DIR/cache" \
        XDG_CONFIG_HOME="$HOME_DIR/config" \
        ARK_ALLOW_INSECURE_HTTP=1 \
        ARK_DOWNLOAD_TOOL=curl \
        WINIT_UNIX_BACKEND=x11 \
        WAYLAND_DISPLAY= \
        WINIT_X11_SCALE_FACTOR=1 \
        RUST_LOG=info \
        "$BIN/ark" "$HOME_DIR" > "$TMP/app.log" 2>&1 &
    APP_PID=$!
    wait_for_log "$TMP/app.log" 'update helper event: UpdateAvailable'

    WINDOW=
    i=0
    while [ -z "$WINDOW" ]; do
        for candidate in $(xdotool search --name 'ark' 2> /dev/null || true); do
            if [ "$(xdotool getwindowname "$candidate" 2> /dev/null || true)" = "ark — ${HOME_DIR##*/}" ]; then
                WINDOW=$candidate
                break
            fi
        done
        i=$((i + 1))
        [ "$i" -lt 200 ] || {
            xdotool search --name '.*' getwindowname %@ >&2 2> /dev/null || true
            fail "ark window did not appear"
        }
        sleep 0.05
    done
    xdotool windowfocus --sync "$WINDOW"
    sleep 0.2

    # 1024x768 default window: update panel primary action center.
    xdotool mousemove --window "$WINDOW" 929 211 click 1
    wait_for_log "$TMP/app.log" 'update helper event: Staged'
    assert_current 0.1.0-beta

    xdotool mousemove --window "$WINDOW" 929 211 click 1
    wait_for_log "$TMP/app.log" 'update helper event: Installed'
    assert_current 0.1.1
    assert_eq "$("$BIN/ark" --version)" "ark 0.1.1"

    xdotool mousemove --window "$WINDOW" 929 211 click 1
    timeout 10 tail --pid="$APP_PID" -f /dev/null
    wait "$APP_PID"
    APP_PID=
}

assert_current() {
    assert_eq "$(readlink "$DATA/ark/current")" "versions/$1"
}

if [ -n "${DISPLAY:-}" ] && command -v xdotool > /dev/null 2>&1; then
    ui_update
else
    run_updater check > "$TMP/check.out" 2>&1
    grep -F 'event: update-available' "$TMP/check.out" > /dev/null || fail "check did not find 0.1.1"
    run_updater stage > "$TMP/stage.out" 2>&1
    assert_current 0.1.0-beta
    run_updater apply --version 0.1.1 > "$TMP/apply.out" 2>&1
    assert_current 0.1.1
    assert_eq "$("$BIN/ark" --version)" "ark 0.1.1"
fi
assert_eq "$(cat "$HOME_DIR/project.yaml")" "project: unchanged"
run_updater rollback > "$TMP/rollback.out" 2>&1
assert_current 0.1.0-beta
assert_eq "$("$BIN/ark" --version)" "ark 0.1.0-beta"

printf 'corrupt' > "$DIST/latest.json.sig"
if run_updater check > "$TMP/corrupt.out" 2>&1; then
    fail "corrupt manifest signature was accepted"
fi
grep -F 'code: signature-invalid' "$TMP/corrupt.out" > /dev/null || fail "corrupt manifest signature error missing"
assert_current 0.1.0-beta
cp "$TMP/latest.json.sig.good" "$DIST/latest.json.sig"

stop_server
if run_updater check > "$TMP/offline.out" 2>&1; then
    fail "offline check succeeded"
fi
assert_current 0.1.0-beta

cat > "$TMP/slow-server.py" <<'PY'
import http.server
import sys
import time

port = int(sys.argv[1])
directory = sys.argv[2]

class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=directory, **kwargs)

    def copyfile(self, source, outputfile):
        while True:
            chunk = source.read(1024)
            if not chunk:
                return
            outputfile.write(chunk)
            outputfile.flush()
            time.sleep(0.05)

http.server.HTTPServer(("127.0.0.1", port), Handler).serve_forever()
PY
start_server slow
env \
    HOME="$HOME_DIR" \
    XDG_DATA_HOME="$DATA" \
    XDG_BIN_HOME="$BIN" \
    XDG_STATE_HOME="$STATE" \
    XDG_CACHE_HOME="$HOME_DIR/cache" \
    XDG_CONFIG_HOME="$HOME_DIR/config" \
    ARK_ALLOW_INSECURE_HTTP=1 \
    ARK_DOWNLOAD_TOOL=curl \
    "$UPDATER" stage > "$TMP/interrupted.out" 2>&1 &
DOWNLOAD_PID=$!
i=0
while ! grep -F 'event: download-start' "$TMP/interrupted.out" > /dev/null 2>&1; do
    i=$((i + 1))
    [ "$i" -lt 200 ] || fail "interrupted download did not start"
    sleep 0.02
done
kill "$DOWNLOAD_PID" 2> /dev/null || true
wait "$DOWNLOAD_PID" 2> /dev/null || true
DOWNLOAD_PID=
assert_current 0.1.0-beta
assert_eq "$("$BIN/ark" --version)" "ark 0.1.0-beta"

echo "installed update smoke passed"
