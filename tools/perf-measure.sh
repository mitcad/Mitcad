#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Measures the application on a design: opening it, the recompute after a
# parameter change and after setting it back (cache hits) and undo, the
# display of the bodies (tessellation and selection structures), hover
# detection, frames while orbiting and the UI's refresh after a change. The
# numbers are the app's own "Timing <step>: <ms> ms" lines
# (MITCAD_LOG_TIMING=1, app/framework/Timing.hpp) and its "Recomputed"
# lines; the wall clock from the process start comes from Qt's message
# pattern.
#
# Usage: tools/perf-measure.sh design.mitcad parameter value original [log]
#   value and original are numbers (mm or radians, as --set takes them).
# Runs headless on Xvfb (ui-test-lib.sh). Use a release build for numbers:
#   UI_APP=build/rel/app/mitcad tools/perf-measure.sh ...
# A software renderer (Xvfb's Mesa llvmpipe) makes frames slower than on a
# GPU; tessellation, picking, recompute and refresh do not depend on it.
# The result store (P7d) is off unless MITCAD_RESULT_STORE names a folder:
# run twice with the same folder to measure opening with stored results.

store=${MITCAD_RESULT_STORE:-off}
source "$(dirname "$0")/ui-test-lib.sh"
export MITCAD_RESULT_STORE=$store

PROJECT=$1
PARAM=$2
VALUE=$3
ORIGINAL=$4
OUT=${5:-}
export MITCAD_LOG_TIMING=1
# As a user runs it: without the UI tests' logs of pick places and volumes.
unset MITCAD_LOG_PICKS MITCAD_LOG_VOLUMES
export UI_GDB=0 UI_WINDOW_WAIT=300
export QT_MESSAGE_PATTERN='[%{time process}] %{message}'

# stats "regex with one number group" [from-line]: count, median, max (ms)
# of the matching lines logged since the line number.
stats() {
  tail -n +"${2:-1}" "$UI_LOG" | sed -n "s/.*$1.*/\\1/p" | sort -g |
    awk '{ v[NR] = $1 } END { if (NR == 0) { print "-"; exit }
      printf "%d x, median %.1f, max %.1f\n", NR, v[int((NR + 1) / 2)], v[NR] }'
}

# last "regex with one group": the last number the pattern captures.
last() { sed -n "s/.*$1.*/\\1/p" "$UI_LOG" | tail -1; }

line_count() { wc -l < "$UI_LOG"; }

ui_start_display

echo "=== Opening $PROJECT, then --set $PARAM=$VALUE and back to $ORIGINAL"
ui_start_app --open "$PROJECT" --set "$PARAM=$VALUE" --set "$PARAM=$ORIGINAL"
for _ in $(seq 1 300); do
  [ "$(grep -c 'Recomputed' "$UI_LOG")" -ge 3 ] && break
  sleep 1
done
sleep 3
opened=$(grep -m1 -o '^\[ *[0-9.]*\] Opened' "$UI_LOG" | tr -dc '0-9.')
bodies=$(grep -m1 'Bodies shown:' "$UI_LOG" | tr ',' '\n' | wc -l)
mapfile -t recomputes < <(grep -o 'Recomputed [0-9]* feature(s) in [0-9.]* ms' "$UI_LOG")
open_ms=$(last 'Timing open: \([0-9.]*\) ms')
load_ms=$(last 'Timing open load: \([0-9.]*\) ms')
display=$(sed -n 's/.*Timing display: //p' "$UI_LOG" | sort -g -r | head -1)
first_refresh=$(sed -n 's/.*Timing refresh: //p' "$UI_LOG" | sort -g -r | head -1)

echo "--- Hover: the cursor over a 6 x 6 grid of the view"
from=$(line_count)
for y in 20 32 44 56 68 80; do
  for x in 20 32 44 56 68 80; do
    xdotool mousemove $(ui_view_at "$x" "$y")
    sleep 0.25
  done
done
sleep 1
hover=$(stats 'Timing hover: \([0-9.]*\) ms' "$from")

echo "--- Orbit: Shift + middle drag"
from=$(line_count)
xdotool keydown shift
ui_view_drag 2 30 50 70 40
ui_view_drag 2 70 40 40 60
xdotool keyup shift
sleep 1
frames=$(stats 'Timing frame: \([0-9.]*\) ms' "$from")

echo "--- Undo and redo of the parameter changes (Ctrl+Z, Ctrl+Y)"
from=$(line_count)
ui_key ctrl+z
sleep 3
ui_key ctrl+y
sleep 3
undo=$(tail -n +"$from" "$UI_LOG" | grep -o 'Recomputed [0-9]* feature(s) in [0-9.]* ms' | paste -sd ';')
refresh=$(stats 'Timing refresh: \([0-9.]*\) ms' "$from")
title=$(stats 'Timing title: \([0-9.]*\) ms' "$from")
ui_crashed && ui_fail "the app crashed"

{
  echo "Design: $(basename "$PROJECT"), $bodies bodies shown"
  echo "Opened after (process start): ${opened:-?} s"
  echo "Open (read, recompute, show): ${open_ms:-?} ms; reading the file ${load_ms:-?} ms"
  echo "Result store: $(grep -m1 -o 'Result store: restored.*' "$UI_LOG" || echo "off")"
  echo "Recomputes: ${recomputes[*]}"
  echo "Display (slowest: the opened design): $display"
  echo "Refresh (slowest: the opened design): $first_refresh"
  echo "Hover: $hover"
  echo "Frames while orbiting: $frames"
  echo "Undo, redo: $undo"
  echo "Refresh after undo/redo: $refresh; title: $title"
  echo "Refresh parts (last):"
  echo "  display: $(last 'Timing display: \([0-9.]*\) ms') ms;" \
    "instances: $(last 'Timing bodies instances: \([0-9.]*\) ms') ms;" \
    "threads: $(last 'Timing bodies threads: \([0-9.]*\) ms') ms"
  for part in snapshot bodies sketches profiles 'sketch entities' datums browser actions; do
    echo "  $part: $(last "Timing refresh $part: \\([0-9.]*\\) ms") ms"
  done
} | tee ${OUT:+"$OUT"}
[ -n "$OUT" ] && cp "$UI_LOG" "${OUT%.*}.log"
ui_finish "performance measurement"
