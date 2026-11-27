#!/bin/sh
# Root source-installer regression tests. These use an isolated HOME/XDG tree
# and a fixture binary, so they never touch the user's real installation.
set -eu

ROOT=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d "${TMPDIR:-/tmp}/ark-source-installer-test.XXXXXX")
trap 'rm -rf "$TMP"' 0 1 2 15

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

cat > "$TMP/ark-under-test" <<'EOF'
#!/bin/sh
if [ "${1-}" != "--version" ]; then
    echo "fixture only supports --version" >&2
    exit 2
fi
echo "ark 0.1.0-beta-icons"
EOF
chmod 755 "$TMP/ark-under-test"

mkdir -p "$TMP/bin"
cat > "$TMP/bin/cargo" <<'EOF'
#!/bin/sh
if [ "${ARK_TEST_CARGO_FAIL:-0}" = "1" ]; then
    echo "cargo must not run when ARK_SKIP_BUILD=1" >&2
    exit 99
fi
printf '%s\n' "$*" > "$ARK_TEST_CARGO_LOG"
EOF
chmod 755 "$TMP/bin/cargo"

run_installer() {
    env \
        HOME="$TEST_HOME" \
        XDG_DATA_HOME="$TEST_DATA" \
        XDG_BIN_HOME="$TEST_BIN" \
        XDG_CONFIG_HOME="$TEST_HOME/.config" \
        XDG_STATE_HOME="$TEST_HOME/.local/state" \
        XDG_CACHE_HOME="$TEST_HOME/.cache" \
        PATH="$TMP/bin:/usr/bin:/bin" \
        "$@"
}

TEST_HOME="$TMP/home"
TEST_DATA="$TEST_HOME/.local/share"
TEST_BIN="$TEST_HOME/local bin"
mkdir -p "$TEST_HOME"

if ! run_installer \
    ARK_SKIP_BUILD=1 \
    ARK_BINARY="$TMP/ark-under-test" \
    ARK_TEST_CARGO_FAIL=1 \
    ARK_TEST_CARGO_LOG="$TMP/cargo-ran" \
    "$ROOT/install.sh" --user > "$TMP/install.out" 2>&1; then
    cat "$TMP/install.out" >&2
    fail "source installer failed"
fi

[ ! -e "$TMP/cargo-ran" ] || fail "installer invoked cargo despite ARK_SKIP_BUILD=1"
[ -x "$TEST_BIN/ark" ] || fail "fixture binary was not installed"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta-icons"

DESKTOP="$TEST_DATA/applications/ark.desktop"
ICON="$TEST_DATA/icons/hicolor/256x256/apps/ark.png"
assert_file "$DESKTOP"
assert_file "$ICON"
if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$DESKTOP" > /dev/null
fi
cmp -s "$ROOT/assets/icons/ark.png" "$ICON" || fail "installed desktop icon differs from the bundled icon"
grep -Fx "Icon=ark" "$DESKTOP" > /dev/null || fail "desktop entry has no Icon=ark"
grep -Fx "StartupWMClass=ark" "$DESKTOP" > /dev/null || fail "desktop entry has no StartupWMClass"
grep -F "Exec=\"$TEST_BIN/ark\" %f" "$DESKTOP" > /dev/null || fail "desktop Exec does not quote the installed path"
grep -F "Installed version: ark 0.1.0-beta-icons" "$TMP/install.out" > /dev/null || fail "installer did not verify the installed version"

# The default path still builds, but does so reproducibly with Cargo's lockfile.
TEST_HOME="$TMP/build-home"
TEST_DATA="$TEST_HOME/.local/share"
TEST_BIN="$TEST_HOME/local bin"
mkdir -p "$TEST_HOME"

if ! run_installer \
    ARK_BINARY="$TMP/ark-under-test" \
    ARK_TEST_CARGO_LOG="$TMP/cargo-args" \
    "$ROOT/install.sh" --user > "$TMP/build-install.out" 2>&1; then
    cat "$TMP/build-install.out" >&2
    fail "source installer build path failed"
fi

assert_file "$TMP/cargo-args"
assert_eq "$(cat "$TMP/cargo-args")" "build --release --locked"

echo "source installer tests passed"
