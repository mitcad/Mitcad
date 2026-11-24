#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/resources/layout/default.json
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-mill-turn.mitcad-ui tools/ui-scripts/cam-mill-turn.json
# CAM mill-turn in the CNC group (mitcad#2067) on macOS, through the test
# driver (tools/ui-scripts/cam-mill-turn.mitcad-ui, the same script as the
# Linux and Windows tests'): a lathe setup with live tools, Turn Facing and
# a Drill on the face from their panels, a pocket on the face and a cross
# hole on the diameter, Generate, Simulate and Post Process… to the Fanuc
# turning dialect, whose program is read here (the C axis on and off,
# feeds per minute, the clamp, polar interpolation, G87, the end).
# Usage: tools/ui-macos-cam-mill-turn-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-cam-mill-turn}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/cam-mill-turn.json" --save "$UI_OUT/files/cam-mill-turn.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 MITCAD_CAM_LIBRARY_DIR=$UI_OUT/files/cam-library \
  MITCAD_TEST_INPUT=$here/ui-scripts/cam-mill-turn.mitcad-ui \
  ui_run_app cam-mill-turn --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$UI_OUT/files/cam-mill-turn.mitcad"
ui_expect_log 'TestDriver: done' 'a lathe setup, turning and live tool operations, generate, simulate and post'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
FILE=$UI_OUT/files/setup1.nc
if [ ! -s "$FILE" ]; then
  ui_fail "no program written"
else
  [ "$(sed -n 1p "$FILE")" = "%" ] || ui_fail "the program does not start with the tape mark"
  grep -qx 'G21 G18 G40 G99' "$FILE" || ui_fail "no millimetres and feeds per revolution in the header"
  for line in 'G96 S200 M3' M5 M154 'G28 H0.' G98 'M133 P2000' M14 M15 G17 G12.1 G13.1 M135 M155 G99; do
    grep -qxF "$line" "$FILE" || ui_fail "no '$line' in the program"
  done
  grep -qE '^G0 X[0-9.]+ C-?[0-9.]+$' "$FILE" || ui_fail "the C axis is not indexed for the hole on the face"
  grep -qE '^G87 Z-5[.] C0[.] X30[.] R-?[0-9.]+ F[0-9.]+$' "$FILE" || ui_fail "no side drilling cycle"
  [ "$(tail -n 2 "$FILE" | tr '\n' '|')" = "M30|%|" ] || ui_fail "the program does not end with M30 and the tape mark"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
