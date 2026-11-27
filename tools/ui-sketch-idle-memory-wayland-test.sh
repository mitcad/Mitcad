#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewPaintLayer.cpp app/view/ViewPaintLayer.hpp app/view/ReplacedPictures.hpp
# check-all sources: app/OcctViewer.cpp app/sketch/SketchOverlay.cpp app/sketch/SketchController.cpp
# check-all sources: app/framework/FloatingLayout.cpp app/framework/MessageBand.cpp app/framework/DisplayPlatform.cpp
# check-all sources: app/main.cpp app/framework/TestDriver.cpp app/framework/TestDriverPointer.cpp tools/cli
# Edit Sketch left alone keeps its memory on native Wayland (mitcad#2489,
# the Wayland side of mitcad#2453 and tools/ui-sketch-idle-memory-test.sh).
# MitCad runs on Qt's Wayland platform (MITCAD_WAYLAND=1) in a headless
# Weston compositor of the test's own (its socket in the test's folder under
# build/, no DISPLAY: never on the desktop), drawing with Mesa's software
# renderer, so the view's textures are in the app's own memory as on Xvfb.
# The design is the X11 test's (mitcad-cli: Sketch1, a 40 x 25 rectangle with
# its two dimensions and a free circle, Extrude1 of the rectangle).
#
# A headless Weston has no input devices, so the in-process test driver
# (MITCAD_TEST_INPUT, app/framework/TestDriver.hpp) gives the input: the
# double-click on Sketch1 in the timeline and the pointer's moves over the
# view, as Qt events to the widgets. The phases, as the X11 test's:
#   - Edit Sketch1, the hint faded; left alone with the pointer last moved
#     over the view: no frame of the view, no paint of the sketch overlay,
#     the RSS grows less than 8 MB.
#   - Small moves of the pointer over the view: each shows another place
#     over the badge, so the overlay is painted again, a new picture of the
#     view's size with a texture of its own; the replaced pictures' textures
#     are let go by the view's full frames (mitcad#2437,
#     view/ReplacedPictures.hpp): at rest afterwards the RSS is less than
#     120 MB above what it was before the moves.
#   - Left alone again: no paint of the overlay, the RSS grows less than
#     8 MB.
# Every SAMPLE seconds the app's RSS, the system's available memory and the
# counts of frames and overlay paints so far are written to a table
# (build/sketch-idle-wayland-<layout>[-trace].tsv, overwritten by the next
# run). A run stops (and fails) before the system runs short: when the
# available memory falls under 2 GiB or the RSS passes 6 GiB.
#
# Usage: tools/ui-sketch-idle-memory-wayland-test.sh [--trace] [--chrome=floating|docked]
#   (default)  the UI test: 20 s left alone, 300 moves, 10 s left alone
#   --trace    the long trace of mitcad#2489: 240 s left alone, 400 moves,
#              60 s left alone (repeat it on a change of the view's painting)
#   --chrome   the window's layout (default floating)
# Without Weston (apt-get install weston) the test notes so and passes.

set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)

TRACE=0
CHROME=floating
for argument in "$@"; do
  case $argument in
  --trace) TRACE=1 ;;
  --chrome=floating | --chrome=docked) CHROME=${argument#--chrome=} ;;
  *)
    echo "usage: $0 [--trace] [--chrome=floating|docked]" >&2
    exit 2
    ;;
  esac
done
if [ "$TRACE" = 1 ]; then
  IDLE=240 MOVES=400 AFTER=60
else
  IDLE=20 MOVES=300 AFTER=10
fi
SAMPLE=${UI_SAMPLE_SECONDS:-2}

. "$here/ui-test-lib.sh"

if ! command -v weston > /dev/null; then
  echo "note: weston is not installed (apt-get install weston): native Wayland not tested"
  echo "PASS: UI sketch idle memory on native Wayland (not run here)"
  exit 0
fi

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
# The test's folder is the compositor's runtime folder too (mktemp: mode
# 0700, as Wayland wants it): its socket is $WORK/wl-0.
WORK=$(mktemp -d "$ROOT/build/ui-sketch-idle-wayland.XXXXXX")
FILE=$WORK/idle.mitcad
TABLE=$ROOT/build/sketch-idle-wayland-$CHROME$([ "$TRACE" = 1 ] && echo -trace).tsv
WESTON_PID=""
stop_weston() {
  if [ -n "$WESTON_PID" ]; then
    kill "$WESTON_PID" 2> /dev/null
    wait "$WESTON_PID" 2> /dev/null
  fi
  WESTON_PID=""
}
trap 'ui_cleanup; stop_weston; rm -rf -- "${WORK:?}"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 25},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [70, 10], "diameter": 12},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The driver's script. Its two waits for a file (go-moves, go-end) hand the
# phases' timing to this shell, which samples meanwhile.
{
  echo "expect-ever 'Opened .*idle\\.mitcad' 60"
  echo "mark"
  echo "dblclick-logged 'Timeline Sketch1 at'"
  echo "wait-log 'Editing sketch F1 \\(Sketch1\\)' 30"
  echo "wait-log \"Sketch badge 'Sketch1: \" 10"
  echo "view-pointer move 60 40 none"
  # The sketch's hint fades after 6 s: in the Floating layout to the message
  # band's stub, whose card moves (one paint of the overlay, which keeps
  # clear of it).
  if [ "$CHROME" = floating ]; then
    echo "wait-log 'Message band stub: hint Sketch' 20"
    echo "sleep 1000"
  else
    echo "sleep 8000"
  fi
  echo "expect-file '$WORK/go-moves' $((IDLE + 600))"
  # Moves of about a pixel (0.08 % of the view's width), 60 to a row, rows
  # a few pixels apart.
  awk -v n="$MOVES" 'BEGIN { for (i = 1; i <= n; i++) printf "view-pointer move %.2f %.2f none\n", 60 + (i % 60) * 0.08, 40 + int(i / 60) * 0.4 }'
  echo "sleep 2000"
  echo "expect-file '$WORK/go-end' $((AFTER + 600))"
  echo "quit"
} > "$WORK/idle.mitcad-ui"

# Weston, headless, with its software renderer; no screen blanking (an idle
# output would stop the frames a client waits for).
XDG_RUNTIME_DIR=$WORK env -u DISPLAY -u WAYLAND_DISPLAY weston --backend=headless --renderer=pixman \
  --width=1600 --height=1000 --socket=wl-0 --idle-time=0 --no-config --shell="${UI_WESTON_SHELL:-desktop}" \
  --log="$WORK/weston.log" > /dev/null 2>&1 &
WESTON_PID=$!
for _ in $(seq 1 100); do
  [ -S "$WORK/wl-0" ] && break
  kill -0 "$WESTON_PID" 2> /dev/null || break
  sleep 0.1
done
[ -S "$WORK/wl-0" ] || { cat "$WORK/weston.log"; ui_fail "Weston did not start"; }
echo "ok   Weston $(weston --version 2> /dev/null | awk '{print $2}'), headless, socket $WORK/wl-0"

# The app on Qt's Wayland platform, OpenGL through Mesa's software renderer
# (its own EGL vendor when glvnd has several: another vendor's driver would
# keep the textures elsewhere).
export MITCAD_LOG_TIMING=1
export MITCAD_TEST_INPUT=$WORK/idle.mitcad-ui
export MITCAD_TEST_STEP_MS=30
export MITCAD_TEST_SETTLE_MS=0
export LIBGL_ALWAYS_SOFTWARE=1
for vendor in /usr/share/glvnd/egl_vendor.d/50_mesa.json /etc/glvnd/egl_vendor.d/50_mesa.json; do
  if [ -f "$vendor" ]; then
    export __EGL_VENDOR_LIBRARY_FILENAMES=$vendor
    break
  fi
done
XDG_RUNTIME_DIR=$WORK WAYLAND_DISPLAY=wl-0 MITCAD_WAYLAND=1 env -u DISPLAY -u QT_QPA_PLATFORM -u QT_XCB_GL_INTEGRATION \
  "$UI_APP" --no-recovery --ui=qt --no-mcp --chrome="$CHROME" --open "$FILE" > "$UI_LOG" 2>&1 &
UI_RUNNER=$!

# count pattern [from]: the lines of the log from line `from` on (1 by
# default) that start with the pattern.
count() { tail -n +"${2:-1}" "$UI_LOG" | grep -c "^$1"; }
# rss: the app's resident memory in kB (0 once it has ended).
rss() {
  local resident
  resident=$(awk '/^VmRSS:/ {print $2}' "/proc/$UI_RUNNER/status" 2> /dev/null)
  echo "${resident:-0}"
}
available() { awk '/^MemAvailable:/ {print $2}' /proc/meminfo; }
STARTED=$(date +%s)
printf 'seconds\tphase\trss_kB\tavailable_kB\tframes\toverlay_paints\n' > "$TABLE"
# sample phase: one row of the table; stops the run when memory runs short.
sample() {
  local now resident free
  now=$(($(date +%s) - STARTED))
  resident=$(rss)
  free=$(available)
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$now" "$1" "$resident" "$free" \
    "$(count "Timing frame:")" "$(count "Timing view paint layer")" >> "$TABLE"
  if [ "$free" -lt 2097152 ] || [ "$resident" -gt 6291456 ]; then
    ui_stop_app
    ui_fail "stopped in phase $1 at $now s: RSS $resident kB, available $free kB (table: $TABLE)"
  fi
}
# wait_step pattern seconds phase: samples until the driver has reached its
# step matching the pattern ("TestDriver: line <n>: <step>").
wait_step() {
  local until=$(($(date +%s) + $2))
  while ! grep -q "^TestDriver: line [0-9]*: $1" "$UI_LOG"; do
    ui_crashed && ui_fail "the app ended in phase $3"
    grep -q "^TestDriver: FAIL" "$UI_LOG" && ui_fail "the test driver failed in phase $3"
    [ "$(date +%s)" -lt "$until" ] || ui_fail "phase $3 did not end in $2 s"
    sample "$3"
    sleep "$SAMPLE"
  done
}
# hold seconds phase: samples for that long.
hold() {
  local until=$(($(date +%s) + $1))
  while [ "$(date +%s)" -lt "$until" ]; do
    ui_crashed && ui_fail "the app ended in phase $2"
    sample "$2"
    sleep "$SAMPLE"
  done
  sample "$2"
}

echo "--- Edit Sketch1 ($CHROME), the hint faded"
wait_step "expect-file .*go-moves" 180 start
ui_expect_log "Display platform: wayland" "MitCad runs on native Wayland"
ui_expect_log "Editing sketch F1 (Sketch1)" "Sketch1 edited"
grep -m 1 "^View composition: " "$UI_LOG"
grep "^View area " "$UI_LOG" | tail -1

echo "--- Left alone for $IDLE s"
mark=$(($(wc -l < "$UI_LOG") + 1))
before=$(rss)
hold "$IDLE" idle
after=$(rss)
frames=$(count "Timing frame:" "$mark")
paints=$(count "Timing view paint layer" "$mark")
[ "$frames" = 0 ] || ui_fail "the view drew $frames frame(s) while the sketch was left alone: $(tail -n +"$mark" "$UI_LOG" | grep -v '^Timing' | head -5)"
[ "$paints" = 0 ] || ui_fail "the overlay was painted $paints time(s) while the sketch was left alone"
[ $((after - before)) -lt 8192 ] || ui_fail "the RSS grew from $before kB to $after kB while the sketch was left alone"
echo "ok   no frame, no paint, RSS $before kB -> $after kB"

echo "--- $MOVES small moves of the pointer"
mark=$(($(wc -l < "$UI_LOG") + 1))
before=$(rss)
touch "$WORK/go-moves"
wait_step "expect-file .*go-end" $((MOVES / 5 + 120)) moves
sample moves
after=$(rss)
paints=$(count "Timing view paint layer" "$mark")
frames=$(count "Timing frame:" "$mark")
peak=$(awk -F'\t' '$2 == "moves" && $3 > peak { peak = $3 } END { print peak + 0 }' "$TABLE")
[ "$paints" -ge 50 ] || ui_fail "the overlay was painted only $paints time(s) for $MOVES moves: the moves did not show their places"
[ $((after - before)) -lt 122880 ] ||
  ui_fail "the RSS grew from $before kB to $after kB over $paints paints of the overlay: the replaced pictures' textures stayed"
echo "ok   $paints paints of the overlay, $frames frames, RSS $before kB -> $after kB (at most $peak kB)"

echo "--- Left alone again for $AFTER s"
mark=$(($(wc -l < "$UI_LOG") + 1))
before=$(rss)
hold "$AFTER" after
after=$(rss)
paints=$(count "Timing view paint layer" "$mark")
frames=$(count "Timing frame:" "$mark")
[ "$paints" = 0 ] || ui_fail "the overlay was painted $paints time(s) while the sketch was left alone after the moves"
[ $((after - before)) -lt 8192 ] || ui_fail "the RSS grew from $before kB to $after kB while left alone after the moves"
echo "ok   no paint, $frames frames, RSS $before kB -> $after kB"

touch "$WORK/go-end"
for _ in $(seq 1 100); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.1
done
grep -q "^TestDriver: quit" "$UI_LOG" || ui_fail "the test driver did not reach its end"
echo "the table: $TABLE ($(($(wc -l < "$TABLE") - 1)) samples, every $SAMPLE s)"
if [ "$TRACE" = 1 ]; then
  cp -f "$UI_LOG" "${TABLE%.tsv}.app.log" && echo "the application's log: ${TABLE%.tsv}.app.log"
fi
awk -F'\t' 'NR > 1 { if (!min || $4 < min) min = $4; if ($3 > max) max = $3 } END { printf "RSS at most %d kB, available memory at least %d kB\n", max, min }' "$TABLE"
ui_finish "UI sketch idle memory on native Wayland ($CHROME)"
