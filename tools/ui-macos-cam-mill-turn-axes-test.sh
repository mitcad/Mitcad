#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/resources/layout/default.json
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-mill-turn-axes.mitcad-ui tools/ui-scripts/cam-mill-turn-axes.json
# CAM mill-turn with a Y axis, a B head and a sub-spindle (mitcad#2319) on
# macOS, through the test driver (tools/ui-scripts/cam-mill-turn-axes.mitcad-ui,
# the same script as the Linux and Windows tests'): a lathe setup handing
# over to the sub-spindle, a flat with the Y axis, a hole on a tilted
# plane, a sketch wrapped onto the diameter, the part-off and the back on
# the sub-spindle, Generate, Simulate (the stock in 3D) and Post Process…
# to the Fanuc turning dialect, whose program is read here (G19, the
# tilted working plane, G7.1, the hand-over, G55, the sub-spindle's
# spindle, the end).
# Usage: tools/ui-macos-cam-mill-turn-axes-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-cam-mill-turn-axes}"
UI_TIMEOUT=400
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/cam-mill-turn-axes.json" --save "$UI_OUT/files/cam-mill-turn-axes.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 MITCAD_CAM_LIBRARY_DIR=$UI_OUT/files/cam-library \
  MITCAD_TEST_INPUT=$here/ui-scripts/cam-mill-turn-axes.mitcad-ui \
  ui_run_app cam-mill-turn-axes --ui=qt --no-native-dialogs --no-recovery --no-mcp \
  --open "$UI_OUT/files/cam-mill-turn-axes.mitcad"
ui_expect_log 'TestDriver: done' 'a lathe setup with a hand-over, Y, B and wrapped live tools, generate, simulate and post'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
FILE=$UI_OUT/files/setup1.nc
if [ ! -s "$FILE" ]; then
  ui_fail "no program written"
else
  [ "$(sed -n 1p "$FILE")" = "%" ] || ui_fail "the program does not start with the tape mark"
  for line in M154 G98 'G0 C0.' M14 G19 G18 'G68.2 X0. Y0. Z0. I90. J45. K-90.' G53.1 M107 G69 'G7.1 C15.' \
    'G7.1 C0.' M213 M79 M78 M212 G55 M155 G99; do
    grep -qxF "$line" "$FILE" || ui_fail "no '$line' in the program"
  done
  grep -qE '^G83 X-?[0-9.]+ Y-?[0-9.]+ Z-?[0-9.]+ R-?[0-9.]+( Q[0-9]+)? F[0-9.]+$' "$FILE" ||
    ui_fail "no drilling cycle on the tilted plane"
  grep -qE '^G1 A-?[0-9.]+ F[0-9.]+$' "$FILE" || ui_fail "the sub-spindle does not come up to the part"
  grep -qE '^G96 S[0-9]+ M203$' "$FILE" || ui_fail "the sub-spindle does not turn the part's back"
  [ "$(tail -n 2 "$FILE" | tr '\n' '|')" = "M30|%|" ] || ui_fail "the program does not end with M30 and the tape mark"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
