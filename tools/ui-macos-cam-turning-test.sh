#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/resources/layout/default.json
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-turning.mitcad-ui tools/ui-scripts/cam-turning.json
# CAM turning in the CNC group (mitcad#1869) on macOS, through the test
# driver (tools/ui-scripts/cam-turning.mitcad-ui, the same script as the
# Linux and Windows tests'): a lathe setup, five turning operations from
# their panels, Generate, Simulate and Post Process… to the Fanuc turning
# dialect, whose program is read here (the program number, the modes,
# G50/G96/G97, G71, G70, G76, X as a diameter, the end).
# Usage: tools/ui-macos-cam-turning-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-cam-turning}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/cam-turning.json" --save "$UI_OUT/files/cam-turning.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 MITCAD_CAM_LIBRARY_DIR=$UI_OUT/files/cam-library \
  MITCAD_TEST_INPUT=$here/ui-scripts/cam-turning.mitcad-ui \
  ui_run_app cam-turning --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/cam-turning.mitcad"
ui_expect_log 'TestDriver: done' 'a lathe setup, five turning operations, generate, simulate and post'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
FILE=$UI_OUT/files/setup1.nc
if [ ! -s "$FILE" ]; then
  ui_fail "no program written"
else
  [ "$(sed -n 1p "$FILE")" = "%" ] || ui_fail "the program does not start with the tape mark"
  [ "$(sed -n 2p "$FILE")" = "O1000 (SETUP 1)" ] || ui_fail "the program's number is not first"
  grep -qx 'G21 G18 G40 G99' "$FILE" || ui_fail "no millimetres and feeds per revolution in the header"
  grep -qx 'G50 S3000' "$FILE" || ui_fail "no spindle limit"
  grep -qx 'G96 S200 M3' "$FILE" || ui_fail "no constant surface speed"
  grep -qE '^G71 U[0-9.]+ R[0-9.]+$' "$FILE" || ui_fail "no roughing cycle"
  grep -qE '^G70 P[0-9]+ Q[0-9]+$' "$FILE" || ui_fail "no finishing cycle"
  grep -qE '^G97 S[1-9][0-9]* M3$' "$FILE" || ui_fail "the thread is not cut at a constant speed"
  grep -qE '^G76 P010060 ' "$FILE" || ui_fail "no threading cycle"
  grep -qE '^G0 X48[.] Z' "$FILE" || ui_fail "X is not written as a diameter"
  [ "$(tail -n 2 "$FILE" | tr '\n' '|')" = "M30|%|" ] || ui_fail "the program does not end with M30 and the tape mark"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
