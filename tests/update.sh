#!/bin/sh
set -eu

ROOT=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d "${TMPDIR:-/tmp}/ark-update-test.XXXXXX")
SERVER_PID=

cleanup() {
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

sign_manifest() {
    openssl pkeyutl -sign -inkey "$TMP/private.pem" -rawin -in "$1" -out "$1.sig"
}

write_manifest() {
    path=$1
    version=$2
    archive_name=$3
    signature_name=$4
    checksum=$5
    cat > "$path" <<EOF
{
  "version": "$version",
  "channel": "stable",
  "published_at": "2026-11-27T00:00:00Z",
  "minimum_version": "0.1.0-beta",
  "critical": false,
  "notes_url": "https://example.com/releases/$version",
  "assets": {
    "x86_64-unknown-linux-gnu": {
      "url": "http://127.0.0.1:$PORT/$archive_name",
      "size": $ARCHIVE_SIZE,
      "sha256": "$checksum",
      "signature": "http://127.0.0.1:$PORT/$signature_name"
    }
  }
}
EOF
}

cd "$ROOT"
cargo build --bin ark-updater --features updater > "$TMP/build-helper.out" 2>&1
UPDATER="$ROOT/target/debug/ark-updater"

printf 'fixture icon\n' > "$TMP/ark.png"
make_binary "$TMP/ark-0.1.0-beta" "0.1.0-beta"
OLD_DIST="$TMP/old-dist"
env \
    ARK_VERSION=0.1.0-beta \
    ARK_ARCH=x86_64 \
    ARK_BINARY="$TMP/ark-0.1.0-beta" \
    ARK_UPDATER="$UPDATER" \
    ARK_ICON="$TMP/ark.png" \
    ARK_SKIP_BUILD=1 \
    ARK_OUTPUT_DIR="$OLD_DIST" \
    sh packaging/build-release.sh > "$TMP/build-old.out" 2>&1

TEST_HOME="$TMP/home"
TEST_DATA="$TEST_HOME/.local/share"
TEST_BIN="$TEST_HOME/.local/bin"
TEST_STATE="$TEST_HOME/.local/state"
TEST_CACHE="$TEST_HOME/.cache"
TEST_CONFIG="$TEST_HOME/.config"
mkdir -p "$TEST_HOME" "$TMP/old-extract"
tar -xzf "$OLD_DIST/ark-0.1.0-beta-linux-x86_64.tar.gz" -C "$TMP/old-extract"
env \
    HOME="$TEST_HOME" \
    XDG_DATA_HOME="$TEST_DATA" \
    XDG_BIN_HOME="$TEST_BIN" \
    XDG_STATE_HOME="$TEST_STATE" \
    XDG_CACHE_HOME="$TEST_CACHE" \
    XDG_CONFIG_HOME="$TEST_CONFIG" \
    sh "$TMP/old-extract/ark-0.1.0-beta-linux-x86_64/install-release.sh" > "$TMP/install-old.out" 2>&1
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

openssl genpkey -algorithm ED25519 -out "$TMP/private.pem" > /dev/null 2>&1
openssl pkey -in "$TMP/private.pem" -pubout -out "$TMP/public.pem" > /dev/null 2>&1

make_binary "$TMP/ark-0.1.1" "0.1.1"
DIST="$TMP/dist"
env \
    ARK_VERSION=0.1.1 \
    ARK_ARCH=x86_64 \
    ARK_BINARY="$TMP/ark-0.1.1" \
    ARK_UPDATER="$UPDATER" \
    ARK_ICON="$TMP/ark.png" \
    ARK_SKIP_BUILD=1 \
    ARK_OUTPUT_DIR="$DIST" \
    sh packaging/build-release.sh > "$TMP/build-new.out" 2>&1
ARCHIVE="$DIST/ark-0.1.1-linux-x86_64.tar.gz"
ARCHIVE_SIZE=$(wc -c < "$ARCHIVE" | tr -d ' ')
REAL_ARCHIVE_SIZE=$ARCHIVE_SIZE
CHECKSUM=$(sha256sum "$ARCHIVE" | awk '{print $1}')
openssl pkeyutl -sign -inkey "$TMP/private.pem" -rawin -in "$ARCHIVE" -out "$DIST/ark-0.1.1.tar.gz.sig"
cp "$DIST/ark-0.1.1.tar.gz.sig" "$DIST/bad.sig"
printf 'not-a-signature' > "$DIST/bad.sig"

mkdir -p "$TMP/malicious/ark-0.1.2"
printf 'escape\n' > "$TMP/malicious/escape"
python3 - "$TMP/malicious/escape" "$DIST/ark-0.1.2-linux-x86_64.tar.gz" <<'PY'
import sys, tarfile
with tarfile.open(sys.argv[2], 'w:gz') as archive:
    archive.add(sys.argv[1], arcname='../escape')
PY
MALICIOUS_SIZE=$(wc -c < "$DIST/ark-0.1.2-linux-x86_64.tar.gz" | tr -d ' ')
MALICIOUS_CHECKSUM=$(sha256sum "$DIST/ark-0.1.2-linux-x86_64.tar.gz" | awk '{print $1}')
openssl pkeyutl -sign -inkey "$TMP/private.pem" -rawin -in "$DIST/ark-0.1.2-linux-x86_64.tar.gz" -out "$DIST/ark-0.1.2.tar.gz.sig"

PORT=$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(('127.0.0.1', 0))
print(s.getsockname()[1])
s.close()
PY
)
export PORT
write_manifest "$DIST/latest.json" 0.1.1 ark-0.1.1-linux-x86_64.tar.gz ark-0.1.1.tar.gz.sig "$CHECKSUM"
write_manifest "$DIST/bad-signature.json" 0.1.1 ark-0.1.1-linux-x86_64.tar.gz bad.sig "$CHECKSUM"
write_manifest "$DIST/bad-checksum.json" 0.1.1 ark-0.1.1-linux-x86_64.tar.gz ark-0.1.1.tar.gz.sig "0000000000000000000000000000000000000000000000000000000000000000"
ARCHIVE_SIZE=$((REAL_ARCHIVE_SIZE + 1))
write_manifest "$DIST/size-mismatch.json" 0.1.1 ark-0.1.1-linux-x86_64.tar.gz ark-0.1.1.tar.gz.sig "$CHECKSUM"
ARCHIVE_SIZE=$((REAL_ARCHIVE_SIZE - 1))
write_manifest "$DIST/oversized.json" 0.1.1 ark-0.1.1-linux-x86_64.tar.gz ark-0.1.1.tar.gz.sig "$CHECKSUM"
ARCHIVE_SIZE=$MALICIOUS_SIZE
write_manifest "$DIST/traversal.json" 0.1.2 ark-0.1.2-linux-x86_64.tar.gz ark-0.1.2.tar.gz.sig "$MALICIOUS_CHECKSUM"
ARCHIVE_SIZE=$MALICIOUS_SIZE
write_manifest "$DIST/downgrade.json" 0.0.9 ark-0.1.2-linux-x86_64.tar.gz ark-0.1.2.tar.gz.sig "$MALICIOUS_CHECKSUM"
for manifest in latest.json bad-signature.json bad-checksum.json size-mismatch.json oversized.json traversal.json downgrade.json; do
    sign_manifest "$DIST/$manifest"
done
write_manifest "$DIST/bad-manifest-signature.json" 0.1.1 ark-0.1.1-linux-x86_64.tar.gz ark-0.1.1.tar.gz.sig "$CHECKSUM"
printf 'not-a-manifest-signature' > "$DIST/bad-manifest-signature.json.sig"

python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$DIST" > "$TMP/server.log" 2>&1 &
SERVER_PID=$!
i=0
while ! curl -fsS "http://127.0.0.1:$PORT/latest.json" > /dev/null 2>&1; do
    i=$((i + 1))
    [ "$i" -lt 100 ] || fail "update server did not start"
    sleep 0.05
done

run_updater() {
    env \
        HOME="$TEST_HOME" \
        XDG_DATA_HOME="$TEST_DATA" \
        XDG_BIN_HOME="$TEST_BIN" \
        XDG_STATE_HOME="$TEST_STATE" \
        XDG_CACHE_HOME="$TEST_CACHE" \
        XDG_CONFIG_HOME="$TEST_CONFIG" \
        ARK_ALLOW_INSECURE_HTTP=1 \
        ARK_UPDATE_PUBLIC_KEY="$TMP/public.pem" \
        ARK_DOWNLOAD_TOOL="${ARK_TEST_DOWNLOAD_TOOL:-curl}" \
        "$UPDATER" "$@"
}

run_updater check --manifest-url "http://127.0.0.1:$PORT/latest.json" > "$TMP/check.out" 2>&1
grep -F 'event: update-available' "$TMP/check.out" > /dev/null || fail "check did not find update"
grep -F 'version: 0.1.1' "$TMP/check.out" > /dev/null || fail "check reported wrong version"

run_updater stage --manifest-url "http://127.0.0.1:$PORT/latest.json" > "$TMP/stage.out" 2>&1
grep -F 'event: ready' "$TMP/stage.out" > /dev/null || fail "stage did not finish"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

run_updater apply --version 0.1.1 > "$TMP/apply.out" 2>&1
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.1"

run_updater rollback > "$TMP/rollback.out" 2>&1
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

DOWNGRADE_STAGE="$TEST_DATA/ark/versions/.stage-downgrade/ark-0.0.9"
mkdir -p "$DOWNGRADE_STAGE"
make_binary "$DOWNGRADE_STAGE/ark" 0.0.9
cat > "$TEST_STATE/ark/updater/staged.json" <<EOF
{
  "version": "0.0.9",
  "path": "$DOWNGRADE_STAGE",
  "channel": "stable",
  "manifest_url": "http://127.0.0.1:$PORT/latest.json"
}
EOF
if run_updater apply --version 0.0.9 > "$TMP/downgrade-apply.out" 2>&1; then
    fail "apply installed a downgrade"
fi
grep -F 'code: staged-release' "$TMP/downgrade-apply.out" > /dev/null || fail "downgrade apply error missing"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"
rm -rf "$TEST_DATA/ark/versions/.stage-downgrade"
rm -f "$TEST_STATE/ark/updater/staged.json"

OUTSIDE_RELEASE="$TMP/outside-release"
mkdir -p "$OUTSIDE_RELEASE"
make_binary "$OUTSIDE_RELEASE/ark" 0.1.2
cat > "$TEST_STATE/ark/updater/staged.json" <<EOF
{
  "version": "0.1.2",
  "path": "$OUTSIDE_RELEASE",
  "channel": "stable",
  "manifest_url": "http://127.0.0.1:$PORT/latest.json"
}
EOF
if run_updater apply --version 0.1.2 > "$TMP/outside-apply.out" 2>&1; then
    fail "apply accepted a staged path outside the install tree"
fi
grep -F 'code: staged-release' "$TMP/outside-apply.out" > /dev/null || fail "outside staged path error missing"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"
rm -f "$TEST_STATE/ark/updater/staged.json"

if run_updater stage --manifest-url "http://127.0.0.1:$PORT/bad-signature.json" > "$TMP/bad-signature.out" 2>&1; then
    fail "invalid signature was accepted"
fi
grep -F 'code: signature-invalid' "$TMP/bad-signature.out" > /dev/null || fail "invalid signature error missing"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

if run_updater check --manifest-url "http://127.0.0.1:$PORT/bad-manifest-signature.json" > "$TMP/bad-manifest-signature.out" 2>&1; then
    fail "invalid manifest signature was accepted"
fi
grep -F 'code: signature-invalid' "$TMP/bad-manifest-signature.out" > /dev/null || fail "invalid manifest signature error missing"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

if run_updater stage --manifest-url "http://127.0.0.1:$PORT/bad-checksum.json" > "$TMP/bad-checksum.out" 2>&1; then
    fail "invalid checksum was accepted"
fi
grep -F 'code: checksum-mismatch' "$TMP/bad-checksum.out" > /dev/null || fail "invalid checksum error missing"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

if run_updater stage --manifest-url "http://127.0.0.1:$PORT/size-mismatch.json" > "$TMP/size-mismatch.out" 2>&1; then
    fail "mismatched download size was accepted"
fi
grep -F 'code: size-mismatch' "$TMP/size-mismatch.out" > /dev/null || fail "download size mismatch error missing"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

if run_updater stage --manifest-url "http://127.0.0.1:$PORT/oversized.json" > "$TMP/oversized.out" 2>&1; then
    fail "oversized download was accepted"
fi
grep -F 'code: download-too-large' "$TMP/oversized.out" > /dev/null || fail "oversized download error missing"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

if run_updater stage --manifest-url "http://127.0.0.1:$PORT/traversal.json" > "$TMP/traversal.out" 2>&1; then
    fail "traversal archive was accepted"
fi
grep -F 'code: unsafe-archive' "$TMP/traversal.out" > /dev/null || fail "traversal error missing"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

run_updater check --manifest-url "http://127.0.0.1:$PORT/downgrade.json" > "$TMP/downgrade.out" 2>&1
grep -F 'event: up-to-date' "$TMP/downgrade.out" > /dev/null || fail "downgrade was offered"

if run_updater stage --manifest-url "http://127.0.0.1:1/missing.json" > "$TMP/offline.out" 2>&1; then
    fail "failed download was accepted"
fi
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

run_updater stage --manifest-url "http://127.0.0.1:$PORT/latest.json" > "$TMP/stage-before-apply-failure.out" 2>&1
chmod 555 "$TEST_STATE/ark"
if run_updater apply --version 0.1.1 > "$TMP/apply-write-failure.out" 2>&1; then
    chmod 755 "$TEST_STATE/ark"
    fail "apply with unwritable metadata succeeded"
fi
chmod 755 "$TEST_STATE/ark"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"
rm -f "$TEST_STATE/ark/updater/staged.json"

mkdir -p "$TEST_DATA/ark/versions/0.0.9"
make_binary "$TEST_DATA/ark/versions/0.0.9/ark" 0.0.9
chmod 555 "$TEST_STATE/ark"
if run_updater rollback > "$TMP/rollback-write-failure.out" 2>&1; then
    chmod 755 "$TEST_STATE/ark"
    fail "rollback with unwritable metadata succeeded"
fi
chmod 755 "$TEST_STATE/ark"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

for version in 0.0.7 0.0.8; do
    mkdir -p "$TEST_DATA/ark/versions/$version"
    make_binary "$TEST_DATA/ark/versions/$version/ark" "$version"
done
run_updater rollback > "$TMP/rollback-retention.out" 2>&1
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.0.9"
if ! run_updater rollback > "$TMP/rollback-retention-second.out" 2>&1; then
    fail "pruning removed the previous version needed for rollback"
fi
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.0.8"

echo "updater integration tests passed"
