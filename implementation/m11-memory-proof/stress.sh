#!/usr/bin/env bash
# M11 memory proof: synthetic kernel-scale repo + X11 interactions + /proc
# measurement. Requires a display, xdotool and spectacle only for screenshots;
# the memory assertion works without them.
#
#   DIRS=80 FILES_PER_DIR=1000 BIG_LINES=200000 ./stress.sh
#
# Expects the release binary at target/release/ark (build it first).
set -euo pipefail

DIRS="${DIRS:-80}"
FILES_PER_DIR="${FILES_PER_DIR:-1000}"
BIG_FILES="${BIG_FILES:-5}"
BIG_LINES="${BIG_LINES:-200000}"
REPO="${REPO:-/tmp/ark-m11-stress}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="$ROOT/target/release/ark"
LOG="/tmp/ark-m11-stress.log"

if [[ ! -x "$BIN" ]]; then
    echo "missing $BIN — run cargo build --release first" >&2
    exit 1
fi

# Optional fake/real agent directory: prepend to PATH and click the first
# launcher (claude) after the tabs open.
if [[ -n "${AGENT_DIR:-}" ]]; then
    export PATH="$AGENT_DIR:$PATH"
fi

if [[ ! -d "$REPO/.git" ]]; then
    echo "generating $REPO (${DIRS}x${FILES_PER_DIR} files + $BIG_FILES big files)"
    rm -rf "$REPO"
    DIRS="$DIRS" FILES="$FILES_PER_DIR" BIG="$BIG_FILES" LINES="$BIG_LINES" REPO="$REPO" python3 - <<'PY'
import os
dirs = int(os.environ["DIRS"]); files = int(os.environ["FILES"])
big = int(os.environ["BIG"]); lines = int(os.environ["LINES"]); repo = os.environ["REPO"]
os.makedirs(repo, exist_ok=True)
for d in range(dirs):
    p = os.path.join(repo, f"d{d:03d}")
    os.makedirs(p, exist_ok=True)
    for f in range(files):
        with open(os.path.join(p, f"f{f:04d}.txt"), "w") as fh:
            fh.write("x\n")
for b in range(big):
    with open(os.path.join(repo, f"big{b:02d}.txt"), "w") as fh:
        for i in range(lines):
            fh.write(f"line {i:07d} of big file {b}\n")
PY
    git -C "$REPO" init -q
    git -C "$REPO" config user.email m11@ark
    git -C "$REPO" config user.name m11
    git -C "$REPO" add -A
    git -C "$REPO" commit -qm "synthetic kernel-scale baseline"
fi

export ARK_MEM_REPORT=1
env -u WAYLAND_DISPLAY DISPLAY="${DISPLAY:-:0}" "$BIN" "$REPO" >"$LOG" 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true' EXIT
sleep 4

if command -v xdotool >/dev/null && [[ -n "${DISPLAY:-}" ]]; then
    W=$(DISPLAY="$DISPLAY" xdotool search --name ark | tail -1 || true)
    if [[ -n "$W" ]]; then
        DISPLAY="$DISPLAY" xdotool windowactivate --sync "$W" || true
        # Expand the first directory (lazy 1000-entry listing), then drag the
        # tree scrollbar to the bottom so the five big root files are visible.
        DISPLAY="$DISPLAY" xdotool mousemove --window "$W" 64 99 click 1
        sleep 0.5
        DISPLAY="$DISPLAY" xdotool mousemove --window "$W" 268 89
        DISPLAY="$DISPLAY" xdotool mousedown 1
        sleep 0.15
        DISPLAY="$DISPLAY" xdotool mousemove --window "$W" 268 550
        sleep 0.35
        DISPLAY="$DISPLAY" xdotool mouseup 1
        sleep 0.2
        # Wheel to the exact bottom (the drag is clamped just shy of the end).
        DISPLAY="$DISPLAY" xdotool mousemove --window "$W" 150 300
        for _ in $(seq 1 20); do DISPLAY="$DISPLAY" xdotool click 5; done
        sleep 0.5
        for y in 385 407 429 451 473; do
            DISPLAY="$DISPLAY" xdotool mousemove --window "$W" 100 "$y"
            DISPLAY="$DISPLAY" xdotool click 1
            sleep 0.08
            DISPLAY="$DISPLAY" xdotool click 1
            sleep 0.25
        done
        # Two terminal tabs, then an optional agent tab, then idle.
        DISPLAY="$DISPLAY" xdotool key ctrl+t; sleep 0.4
        DISPLAY="$DISPLAY" xdotool key ctrl+t; sleep 0.4
        if [[ -n "${AGENT_DIR:-}" ]]; then
            # Four fake launchers place claude at 670..752 in the header.
            DISPLAY="$DISPLAY" xdotool mousemove --window "$W" 711 19 click 1
            sleep 0.8
        fi
    fi
fi
# Wait up to 25s for the idle malloc_trim report (git status/fs events on the
# 80k-file repo can delay idleness on the first run).
for _ in $(seq 1 25); do
    grep -q "memory: trimmed=" "$LOG" && break
    sleep 1
done
sleep 1

RESULT="$(grep -E "VmHWM:|VmRSS:" "/proc/$pid/status" | tr '\n' ' ' || true)"
PSS_LINE="$(grep -E "^Pss:" "/proc/$pid/smaps_rollup" | tr -s ' ' || true)"
echo "M11 stress: $RESULT $PSS_LINE"
PSS_KIB="$(awk '{print $2}' <<<"$PSS_LINE")"
awk -v line="$RESULT" -v pss="$PSS_KIB" 'BEGIN {
    split(line, a, /[ \t]+/);
    rss=0;
    for (i in a) if (a[i]=="VmRSS:") rss=a[i+1];
    if (rss == 0) { print "could not parse VmRSS"; exit 2 }
    printf "VmRSS = %.1f MiB, PSS = %.1f MiB\n", rss/1024, pss/1024;
    if (rss > 100*1024 || pss > 100*1024) {
        print "FAIL: over the 100 MiB budget"; exit 1
    }
    print "PASS: RSS and PSS under the 100 MiB budget";
}'
echo "--- idle trim ---"
grep "memory: trimmed=" "$LOG" | tail -1 || echo "no trim log yet"
echo "--- screenshots/interaction log ---"
tail -20 "$LOG"
