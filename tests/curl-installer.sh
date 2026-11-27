#!/bin/sh
set -eu

ROOT=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d "${TMPDIR:-/tmp}/ark-curl-installer-test.XXXXXX")
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

assert_file() {
    [ -f "$1" ] || fail "expected file: $1"
}

assert_eq() {
    [ "$1" = "$2" ] || fail "expected '$2', got '$1'"
}

FIXTURE_BIN="$TMP/ark"
cat > "$FIXTURE_BIN" <<'EOF'
#!/bin/sh
if [ "${1-}" = "--version" ]; then
    echo "ark 0.1.0-beta"
    exit 0
fi
exit 0
EOF
chmod 755 "$FIXTURE_BIN"
cat > "$TMP/ark-updater" <<'EOF'
#!/bin/sh
if [ "${1-}" = "--version" ]; then
    echo "ark-updater 0.1.0-beta"
    exit 0
fi
exit 0
EOF
chmod 755 "$TMP/ark-updater"
printf 'fixture icon\n' > "$TMP/ark.png"

DIST="$TMP/dist"
env \
    ARK_VERSION=0.1.0-beta \
    ARK_ARCH=x86_64 \
    ARK_BINARY="$FIXTURE_BIN" \
    ARK_UPDATER="$TMP/ark-updater" \
    ARK_ICON="$TMP/ark.png" \
    ARK_SKIP_BUILD=1 \
    ARK_OUTPUT_DIR="$DIST" \
    sh "$ROOT/packaging/build-release.sh" > "$TMP/build.out" 2>&1

INSTALLER="$DIST/ark-installer.sh"
ARCHIVE="$DIST/ark-0.1.0-beta-linux-x86_64.tar.gz"
assert_file "$INSTALLER"
assert_file "$ARCHIVE"
grep -F "ARK_VERSION='0.1.0-beta'" "$INSTALLER" > /dev/null || fail "installer does not contain the release version"
grep -F "ARK_ARCH_BAKED='x86_64'" "$INSTALLER" > /dev/null || fail "installer does not contain the release architecture"
grep -F 'ARCHIVE_NAME="ark-$ARK_VERSION-linux-$ARCH.tar.gz"' "$INSTALLER" > /dev/null || fail "installer does not construct the release archive name"
grep -F 'kushagrapokera/ark' "$INSTALLER" > /dev/null || fail "installer does not contain the release repository"

PORT=$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(('127.0.0.1', 0))
print(s.getsockname()[1])
s.close()
PY
)
BASE_URL="http://127.0.0.1:$PORT"
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$DIST" > "$TMP/server.log" 2>&1 &
SERVER_PID=$!

i=0
while ! curl -fsS "$BASE_URL/SHA256SUMS" > /dev/null 2>&1; do
    i=$((i + 1))
    [ "$i" -lt 100 ] || fail "local release server did not start"
    sleep 0.05
done

TEST_HOME="$TMP/home"
TEST_DATA="$TEST_HOME/.local/share"
TEST_BIN="$TEST_HOME/.local/bin"
TEST_STATE="$TEST_HOME/.local/state"
TEST_TMP="$TMP/bootstrap-tmp"
mkdir -p "$TEST_HOME" "$TEST_TMP"

env \
    HOME="$TEST_HOME" \
    XDG_DATA_HOME="$TEST_DATA" \
    XDG_BIN_HOME="$TEST_BIN" \
    XDG_STATE_HOME="$TEST_STATE" \
    XDG_CACHE_HOME="$TEST_HOME/.cache" \
    XDG_CONFIG_HOME="$TEST_HOME/.config" \
    TMPDIR="$TEST_TMP" \
    ARK_BASE_URL="$BASE_URL" \
    ARK_ARCH=x86_64 \
    sh "$INSTALLER" > "$TMP/install.out" 2>&1

assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"
[ -z "$(find "$TEST_TMP" -mindepth 1 -print -quit)" ] || fail "successful install left temporary files"

if command -v wget > /dev/null 2>&1; then
    WGET_HOME="$TMP/wget-home"
    env \
        HOME="$WGET_HOME" \
        XDG_DATA_HOME="$WGET_HOME/.local/share" \
        XDG_BIN_HOME="$WGET_HOME/.local/bin" \
        XDG_STATE_HOME="$WGET_HOME/.local/state" \
        TMPDIR="$TEST_TMP" \
        ARK_BASE_URL="$BASE_URL" \
        ARK_ARCH=x86_64 \
        ARK_DOWNLOAD_TOOL=wget \
        sh "$INSTALLER" > "$TMP/wget.out" 2>&1
    assert_eq "$("$WGET_HOME/.local/bin/ark" --version)" "ark 0.1.0-beta"
fi

CORRUPT_DIST="$TMP/corrupt-dist"
cp -R "$DIST" "$CORRUPT_DIST"
printf 'corruption' >> "$CORRUPT_DIST/ark-0.1.0-beta-linux-x86_64.tar.gz"
kill "$SERVER_PID" 2> /dev/null || true
SERVER_PID=
CORRUPT_PORT=$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(('127.0.0.1', 0))
print(s.getsockname()[1])
s.close()
PY
)
python3 -m http.server "$CORRUPT_PORT" --bind 127.0.0.1 --directory "$CORRUPT_DIST" > "$TMP/corrupt-server.log" 2>&1 &
SERVER_PID=$!
i=0
while ! curl -fsS "http://127.0.0.1:$CORRUPT_PORT/SHA256SUMS" > /dev/null 2>&1; do
    i=$((i + 1))
    [ "$i" -lt 100 ] || fail "corrupt release server did not start"
    sleep 0.05
done
CORRUPT_HOME="$TMP/corrupt-home"
if env \
    HOME="$CORRUPT_HOME" \
    XDG_DATA_HOME="$CORRUPT_HOME/.local/share" \
    XDG_BIN_HOME="$CORRUPT_HOME/.local/bin" \
    XDG_STATE_HOME="$CORRUPT_HOME/.local/state" \
    TMPDIR="$TEST_TMP" \
    ARK_BASE_URL="http://127.0.0.1:$CORRUPT_PORT" \
    ARK_ARCH=x86_64 \
    sh "$INSTALLER" > "$TMP/corrupt.out" 2>&1; then
    fail "corrupt download was installed"
fi
[ ! -e "$CORRUPT_HOME/.local/bin/ark" ] || fail "corrupt download created an ark binary"
[ -z "$(find "$TEST_TMP" -mindepth 1 -print -quit)" ] || fail "failed install left temporary files"

if env \
    HOME="$TMP/unsupported-home" \
    ARK_BASE_URL="$BASE_URL" \
    ARK_ARCH=riscv64 \
    sh "$INSTALLER" > "$TMP/unsupported.out" 2>&1; then
    fail "unsupported architecture was accepted"
fi
grep -F 'unsupported architecture' "$TMP/unsupported.out" > /dev/null || fail "unsupported architecture error is not actionable"

if env \
    HOME="$TMP/no-downloader-home" \
    ARK_BASE_URL="$BASE_URL" \
    ARK_DOWNLOAD_TOOL=missing \
    sh "$INSTALLER" > "$TMP/no-downloader.out" 2>&1; then
    fail "missing downloader was accepted"
fi
grep -F 'curl or wget' "$TMP/no-downloader.out" > /dev/null || fail "missing downloader error is not actionable"

echo "curl installer tests passed"
