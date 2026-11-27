#!/bin/sh
set -eu

ROOT=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d "${TMPDIR:-/tmp}/ark-installer-test.XXXXXX")
trap 'rm -rf "$TMP"' 0 1 2 15

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

assert_file() {
    [ -f "$1" ] || fail "expected file: $1"
}

assert_link() {
    [ -L "$1" ] || fail "expected symlink: $1"
}

assert_eq() {
    [ "$1" = "$2" ] || fail "expected '$2', got '$1'"
}

make_fixture() {
    FIXTURE="$TMP/fixture/ark-0.1.0-beta-linux-x86_64"
    mkdir -p "$FIXTURE/icons"
    cat > "$FIXTURE/ark" <<'EOF'
#!/bin/sh
if [ "${1-}" = "--version" ]; then
    echo "ark 0.1.0-beta"
    exit 0
fi
exit 0
EOF
    chmod 755 "$FIXTURE/ark"
    cat > "$FIXTURE/ark-updater" <<'EOF'
#!/bin/sh
if [ "${1-}" = "--version" ]; then
    echo "ark-updater 0.1.0-beta"
    exit 0
fi
exit 0
EOF
    chmod 755 "$FIXTURE/ark-updater"
    cp "$ROOT/packaging/ark.desktop" "$FIXTURE/ark.desktop"
    cp "$ROOT/packaging/install-release.sh" "$FIXTURE/install-release.sh"
    cp "$ROOT/packaging/uninstall.sh" "$FIXTURE/uninstall.sh"
    cp "$ROOT/README.md" "$FIXTURE/README.md"
    cp "$ROOT/LICENSE" "$FIXTURE/LICENSE"
    cp "$ROOT/CHANGELOG.md" "$FIXTURE/CHANGELOG.md"
    printf 'fixture icon\n' > "$FIXTURE/icons/ark.png"
}

run_installer() {
    env \
        HOME="$TEST_HOME" \
        XDG_DATA_HOME="$TEST_DATA" \
        XDG_BIN_HOME="$TEST_BIN" \
        XDG_CONFIG_HOME="$TEST_CONFIG" \
        XDG_STATE_HOME="$TEST_STATE" \
        XDG_CACHE_HOME="$TEST_CACHE" \
        "$@"
}

make_fixture

TEST_HOME="$TMP/home"
TEST_DATA="$TEST_HOME/.local/share"
TEST_BIN="$TEST_HOME/.local/bin"
TEST_CONFIG="$TEST_HOME/.config"
TEST_STATE="$TEST_HOME/.local/state"
TEST_CACHE="$TEST_HOME/.cache"
mkdir -p "$TEST_HOME"
printf 'user project\n' > "$TEST_HOME/project-marker"

run_installer "$FIXTURE/install-release.sh" > "$TMP/install.out" 2>&1

VERSION_DIR="$TEST_DATA/ark/versions/0.1.0-beta"
assert_file "$VERSION_DIR/ark"
assert_link "$TEST_DATA/ark/current"
assert_eq "$(readlink "$TEST_DATA/ark/current")" "versions/0.1.0-beta"
assert_link "$TEST_BIN/ark"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"
assert_file "$TEST_DATA/applications/ark.desktop"
grep -F "Exec=$TEST_BIN/ark %f" "$TEST_DATA/applications/ark.desktop" > /dev/null || fail "desktop Exec is not absolute"
assert_file "$TEST_DATA/icons/hicolor/256x256/apps/ark.png"
assert_file "$TEST_STATE/ark/install.json"
grep -F '"version": "0.1.0-beta"' "$TEST_STATE/ark/install.json" > /dev/null || fail "install metadata version missing"
grep -F '"arch": "x86_64"' "$TEST_STATE/ark/install.json" > /dev/null || fail "install metadata arch missing"

# Reinstalling the same version must be safe.
run_installer "$FIXTURE/install-release.sh" > "$TMP/reinstall.out" 2>&1
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

# A broken bundle must not replace the working current version.
cat > "$FIXTURE/ark" <<'EOF'
#!/bin/sh
echo "broken bundle" >&2
exit 1
EOF
chmod 755 "$FIXTURE/ark"
if run_installer "$FIXTURE/install-release.sh" > "$TMP/broken.out" 2>&1; then
    fail "broken bundle installed successfully"
fi
assert_eq "$(readlink "$TEST_DATA/ark/current")" "versions/0.1.0-beta"
assert_eq "$("$TEST_BIN/ark" --version)" "ark 0.1.0-beta"

# Upgrading keeps the previous version available for rollback.
cat > "$FIXTURE/ark" <<'EOF'
#!/bin/sh
if [ "${1-}" = "--version" ]; then
    echo "ark 0.1.1"
    exit 0
fi
exit 0
EOF
chmod 755 "$FIXTURE/ark"
run_installer "$FIXTURE/install-release.sh" > "$TMP/upgrade.out" 2>&1
assert_eq "$(readlink "$TEST_DATA/ark/current")" "versions/0.1.1"
assert_file "$TEST_DATA/ark/versions/0.1.0-beta/ark"
assert_file "$TEST_DATA/ark/versions/0.1.1/ark"

# Restore a valid bundle for packaging tests.
cat > "$FIXTURE/ark" <<'EOF'
#!/bin/sh
if [ "${1-}" = "--version" ]; then
    echo "ark 0.1.0-beta"
    exit 0
fi
exit 0
EOF
chmod 755 "$FIXTURE/ark"

if run_installer ARK_ARCH=riscv64 "$FIXTURE/install-release.sh" > "$TMP/arch.out" 2>&1; then
    fail "unsupported architecture was accepted"
fi

if env \
    ARK_VERSION=0.1.0-beta \
    ARK_ARCH=x86_64 \
    ARK_BINARY="$FIXTURE/ark" \
    ARK_UPDATER="$TMP/missing-ark-updater" \
    ARK_ICON="$FIXTURE/icons/ark.png" \
    ARK_SKIP_BUILD=1 \
    ARK_OUTPUT_DIR="$TMP/missing-updater-dist" \
    sh "$ROOT/packaging/build-release.sh" > "$TMP/missing-updater-build.out" 2>&1; then
    fail "release without ark-updater was built"
fi

DIST="$TMP/dist"
env \
    ARK_VERSION=0.1.0-beta \
    ARK_ARCH=x86_64 \
    ARK_BINARY="$FIXTURE/ark" \
    ARK_UPDATER="$FIXTURE/ark-updater" \
    ARK_ICON="$FIXTURE/icons/ark.png" \
    ARK_SKIP_BUILD=1 \
    ARK_OUTPUT_DIR="$DIST" \
    sh "$ROOT/packaging/build-release.sh" > "$TMP/build.out" 2>&1

ARCHIVE="$DIST/ark-0.1.0-beta-linux-x86_64.tar.gz"
CHECKSUMS="$DIST/SHA256SUMS"
assert_file "$ARCHIVE"
assert_file "$CHECKSUMS"
sh "$ROOT/packaging/verify-release.sh" "$ARCHIVE" "$CHECKSUMS" > "$TMP/verify.out" 2>&1

cp "$ARCHIVE" "$TMP/corrupt.tar.gz"
printf 'corruption' >> "$TMP/corrupt.tar.gz"
if sh "$ROOT/packaging/verify-release.sh" "$TMP/corrupt.tar.gz" "$CHECKSUMS" > "$TMP/corrupt.out" 2>&1; then
    fail "corrupt archive passed verification"
fi

# Uninstall removes only ark-created files.
mkdir -p "$TEST_CONFIG/ark" "$TEST_CACHE/ark"
printf 'config\n' > "$TEST_CONFIG/ark/settings"
printf 'cache\n' > "$TEST_CACHE/ark/download"
env \
    HOME="$TEST_HOME" \
    XDG_DATA_HOME="$TEST_DATA" \
    XDG_BIN_HOME="$TEST_BIN" \
    XDG_CONFIG_HOME="$TEST_CONFIG" \
    XDG_STATE_HOME="$TEST_STATE" \
    XDG_CACHE_HOME="$TEST_CACHE" \
    sh "$FIXTURE/uninstall.sh" > "$TMP/uninstall.out" 2>&1

[ ! -e "$TEST_BIN/ark" ] || fail "ark binary link survived uninstall"
[ ! -e "$TEST_DATA/ark" ] || fail "ark data directory survived uninstall"
[ ! -e "$TEST_DATA/applications/ark.desktop" ] || fail "desktop entry survived uninstall"
[ ! -e "$TEST_DATA/icons/hicolor/256x256/apps/ark.png" ] || fail "icon survived uninstall"
assert_file "$TEST_STATE/ark/install.json"
assert_file "$TEST_CONFIG/ark/settings"
assert_file "$TEST_CACHE/ark/download"
assert_file "$TEST_HOME/project-marker"

env \
    HOME="$TEST_HOME" \
    XDG_DATA_HOME="$TEST_DATA" \
    XDG_BIN_HOME="$TEST_BIN" \
    XDG_CONFIG_HOME="$TEST_CONFIG" \
    XDG_STATE_HOME="$TEST_STATE" \
    XDG_CACHE_HOME="$TEST_CACHE" \
    sh "$FIXTURE/uninstall.sh" --purge > "$TMP/purge.out" 2>&1
[ ! -e "$TEST_STATE/ark" ] || fail "state survived --purge"
[ ! -e "$TEST_CONFIG/ark" ] || fail "config survived --purge"
[ ! -e "$TEST_CACHE/ark" ] || fail "cache survived --purge"
assert_file "$TEST_HOME/project-marker"

echo "installer tests passed"
