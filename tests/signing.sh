#!/bin/sh
set -eu

ROOT=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d "${TMPDIR:-/tmp}/ark-signing-test.XXXXXX")
trap 'rm -rf "$TMP"' 0 1 2 15

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

make_binary() {
    path=$1
    version=$2
    name=$3
    cat > "$path" <<EOF
#!/bin/sh
if [ "\${1-}" = "--version" ]; then
    echo "$name $version"
    exit 0
fi
exit 0
EOF
    chmod 755 "$path"
}

openssl genpkey -algorithm ED25519 -out "$TMP/private.pem" > /dev/null 2>&1
openssl pkey -in "$TMP/private.pem" -pubout -out "$TMP/public.pem" > /dev/null 2>&1
make_binary "$TMP/ark" 0.1.1 ark
make_binary "$TMP/ark-updater" 0.1.1 ark-updater
printf 'fixture icon\n' > "$TMP/ark.png"
DIST="$TMP/dist"
env \
    ARK_VERSION=0.1.1 \
    ARK_ARCH=x86_64 \
    ARK_REPO=example/ark \
    ARK_BINARY="$TMP/ark" \
    ARK_UPDATER="$TMP/ark-updater" \
    ARK_ICON="$TMP/ark.png" \
    ARK_SKIP_BUILD=1 \
    ARK_OUTPUT_DIR="$DIST" \
    sh "$ROOT/packaging/build-release.sh" > "$TMP/build.out" 2>&1

sh "$ROOT/packaging/sign-release.sh" "$DIST" 0.1.1 "$TMP/private.pem" 0.1.0 example/ark
ARCHIVE="$DIST/ark-0.1.1-linux-x86_64.tar.gz"
for asset in \
    ark-0.1.1-linux-x86_64.tar.gz \
    ark-0.1.1-linux-x86_64.tar.gz.sig \
    ark-installer.sh \
    ark-installer.sh.sig \
    SHA256SUMS \
    SHA256SUMS.sig \
    latest.json \
    latest.json.sig; do
    [ -f "$DIST/$asset" ] || fail "missing signed release asset: $asset"
done
sh "$ROOT/packaging/verify-release.sh" "$ARCHIVE" "$DIST/SHA256SUMS" "$TMP/public.pem"

cp -R "$DIST" "$TMP/corrupt"
printf 'corruption' > "$TMP/corrupt/ark-0.1.1-linux-x86_64.tar.gz.sig"
if sh "$ROOT/packaging/verify-release.sh" "$TMP/corrupt/ark-0.1.1-linux-x86_64.tar.gz" "$TMP/corrupt/SHA256SUMS" "$TMP/public.pem" > "$TMP/corrupt.out" 2>&1; then
    fail "corrupt archive signature was accepted"
fi

cp -R "$DIST" "$TMP/missing"
rm "$TMP/missing/latest.json.sig"
if sh "$ROOT/packaging/verify-release.sh" "$TMP/missing/ark-0.1.1-linux-x86_64.tar.gz" "$TMP/missing/SHA256SUMS" "$TMP/public.pem" > "$TMP/missing.out" 2>&1; then
    fail "missing manifest signature was accepted"
fi

if grep -R -F 'BEGIN PRIVATE KEY' "$DIST" > /dev/null 2>&1; then
    fail "private key appeared in release artifacts"
fi

cp -R "$DIST" "$TMP/unsafe"
cp "$TMP/private.pem" "$TMP/unsafe/private.pem"
if sh "$ROOT/packaging/sign-release.sh" "$TMP/unsafe" 0.1.1 "$TMP/unsafe/private.pem" 0.1.0 example/ark > "$TMP/unsafe.out" 2>&1; then
    fail "private key inside release directory was accepted"
fi

echo "signed release tests passed"
