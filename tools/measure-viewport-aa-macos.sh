#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Opt-in physical-display diagnostic for mitcad#552; not a ctest benchmark.
# Usage: bash tools/measure-viewport-aa-macos.sh <app> <cli> <output>
# MITCAD_VIEWPORT_SYNC=0 (default): paint submission + frameSwapped intervals.
# MITCAD_VIEWPORT_SYNC=1: include GL completion in paint duration as well.
# Run alone on the physical Mac, without Qt scale overrides. Repeat on a
# verified actual 1x display; QT_SCALE_FACTOR=1 does not establish that.

set -eu
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo "usage: $0 <app> <cli> <output>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
for scale_var in QT_SCALE_FACTOR QT_SCREEN_SCALE_FACTORS QT_DEVICE_PIXEL_RATIO QT_ENABLE_HIGHDPI_SCALING; do
  if printenv "$scale_var" > /dev/null; then
    echo "FAIL: unset $scale_var; the measurement needs the display's actual scale" >&2
    exit 2
  fi
done
export MITCAD_LOG_TIMING=1
export MITCAD_LOG_PICKS=0 MITCAD_LOG_VOLUMES=0
export MITCAD_TEST_STEP_MS=8 MITCAD_TEST_SETTLE_MS=0
export MITCAD_VIEWPORT_SYNC=${MITCAD_VIEWPORT_SYNC:-0}
export MITCAD_UI_REQUIRE_GUI=1
export QT_MESSAGE_PATTERN='%{message}'
UI_TIMEOUT=120

sw_vers > "$UI_OUT/os.txt"
system_profiler SPDisplaysDataType > "$UI_OUT/display.txt"
"$cli" run "$here/ui-fixtures/viewport-dome.json" --save "$UI_OUT/dome.mitcad" > "$UI_OUT/model.log" 2>&1

# Reuse the real pointer driver, one move every two 8 ms ticks. Warm up
# with a full circular orbit, then measure another two identical circles.
# Markers delimit completed input (queued events run before the next step).
script=$UI_OUT/orbit.mitcad-ui
awk 'BEGIN {
  print "wait-idle 60"
  print "expect-ever '\''Viewport timing environment:'\''"
  print "window-resize 1280 820"
  print "key F6"
  print "sleep 500"
  print "capture-view-orientation initial"
  print "view-pointer press 74 52 right"
  for (i = 1; i <= 90; ++i) {
    t = i * 6.283185307179586 / 30
    printf "view-pointer move %.6f %.6f right\n", 62 + 12 * cos(t), 52 + 12 * sin(t)
    if (i == 30) print "set AA_PHASE measured"
    if (i == 45) print "expect-view-orientation ${initial} changed"
  }
  print "set AA_PHASE finished"
  print "view-pointer release 74 52 right"
  print "sleep 500"
  print "expect-view-buttons none"
  print "screenshot-view ${MITCAD_TEST_AA_SHOT}"
}' > "$script"

printf 'mode\tgpu_sync\tmetric\tn\tmedian_ms\tmax_ms\n' > "$UI_OUT/summary.tsv"
for mode in msaa4 msaa2 off ray-aa; do
  # Fresh settings per mode prevent restored window/camera differences.
  rm -rf "$MITCAD_SETTINGS_DIR"
  mkdir -p "$MITCAD_SETTINGS_DIR"
  MITCAD_VIEWPORT_AA=$mode MITCAD_TEST_INPUT=$script MITCAD_TEST_AA_SHOT=$UI_OUT/$mode.png \
    ui_run_app "$mode" --ui=qt --no-recovery --open "$UI_OUT/dome.mitcad"
  if [ "$UI_EXIT" -ne 0 ] || ! grep -q 'TestDriver: done' "$UI_LOG_FILE"; then
    ui_log_tail
    exit 1
  fi
  if [ "$(grep -c 'TestDriver:.*view-pointer move' "$UI_LOG_FILE")" -ne 90 ]; then
    echo "FAIL: $mode did not complete all 30 warmup and 60 measured moves" >&2
    exit 1
  fi
  # Keep raw ordered logs. Frame and swap durations cover different scopes;
  # no presentation-rate or genuine raster FXAA claim is inferred here.
  awk '/TestDriver:.*set AA_PHASE measured/ { measured=1; next }
       /TestDriver:.*set AA_PHASE finished/ { measured=0 }
       measured && /^Timing frame: / { print "paint", $3 }
       measured && /^Timing frame swap: / { print "swap", $4 }' \
      "$UI_LOG_FILE" > "$UI_OUT/$mode.samples"
  for metric in paint swap; do
    awk -v metric="$metric" '$1 == metric { print $2 }' "$UI_OUT/$mode.samples" | sort -n | \
      awk -v mode="$mode" -v sync="$MITCAD_VIEWPORT_SYNC" -v metric="$metric" '
        { samples[NR]=$1 }
        END {
          if (!NR) exit 1
          median = NR % 2 ? samples[(NR+1)/2] : (samples[NR/2]+samples[NR/2+1])/2
          printf "%s\t%s\t%s\t%d\t%.2f\t%.2f\n", mode, sync, metric, NR, median, samples[NR]
        }' >> "$UI_OUT/summary.tsv"
  done
done
cat "$UI_OUT/summary.tsv"
echo "Raw logs, screenshots and display/OS metadata: $UI_OUT"
echo 'ray-aa toggles ray-tracer FSAA; OCCT 8 has no raster FXAA in this viewport.'
