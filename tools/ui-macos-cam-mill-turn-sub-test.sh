#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/resources/layout/default.json
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-mill-turn-sub.mitcad-ui tools/ui-scripts/cam-mill-turn-sub.json
# CAM mill-turn: live tools on the sub-spindle, the B head's and the
# sub-spindle's codes in Machines and Tools…, the sub-spindle's travel in
# the lathe check (mitcad#2375) on macOS, through the test driver
# (tools/ui-scripts/cam-mill-turn-sub.mitcad-ui, the same script as the
# Linux and Windows tests'): Edit Machine… with the sub-spindle's page, a
# lathe setup handing over to the sub-spindle, the back faced, drilled and
# milled there, Generate, Simulate and Post Process… to the Fanuc turning
# dialect, whose program is read here (the sub-spindle's stop, its C axis,
# G19, its axis E, the end), and the lathe check's finding; the
# sub-spindle's body, a zone moving with it, a control that mirrors only Z
# on it and an operation on the main spindle after the sub-spindle's
# (mitcad#2392); the sub-spindle's profile and the turret's body in Edit
# Machine…, the turret running into the sub-spindle's chuck, Simulate
# drawing the sub-spindle where it waits and at the grip (mitcad#2405); a
# fixed zone with its Y, the Turret page with a station's own body, the
# turret running into the fixed zone, Simulate drawing the zones and the
# turret (mitcad#2412).
# Usage: tools/ui-macos-cam-mill-turn-sub-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-cam-mill-turn-sub}"
UI_TIMEOUT=600
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/cam-mill-turn-sub.json" --save "$UI_OUT/files/cam-mill-turn-sub.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 MITCAD_CAM_LIBRARY_DIR=$UI_OUT/files/cam-library \
  MITCAD_TEST_INPUT=$here/ui-scripts/cam-mill-turn-sub.mitcad-ui \
  ui_run_app cam-mill-turn-sub --ui=qt --no-native-dialogs --no-recovery --no-mcp \
  --open "$UI_OUT/files/cam-mill-turn-sub.mitcad"
ui_expect_log 'TestDriver: done' "the sub-spindle's codes, live tools on the sub-spindle, generate, simulate, post and the lathe check"
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
FILE=$UI_OUT/files/setup1.nc
if [ ! -s "$FILE" ]; then
  ui_fail "no program written"
else
  [ "$(sed -n 1p "$FILE")" = "%" ] || ui_fail "the program does not start with the tape mark"
  for line in G55 M206 M264 'G28 H0.' M114 G19 G18 M255 G99 '(DRILL 1)' '(FACING 1)' '(TURN FACING 2)'; do
    grep -qxF "$line" "$FILE" || ui_fail "no '$line' in the program"
  done
  if grep -qxF M154 "$FILE"; then
    ui_fail "the main spindle's C axis in the program"
  fi
  grep -qE '^G1 E-?[0-9.]+ F[0-9.]+$' "$FILE" || ui_fail "the sub-spindle does not come up to the part on its axis E"
  drill=$(grep -nxF '(DRILL 1)' "$FILE" | head -1 | cut -d: -f1)
  stop=$(awk -v from="${drill:-0}" 'NR > from && $0 == "M206" { print NR; exit }' "$FILE")
  on=$(awk -v from="${drill:-0}" 'NR > from && $0 == "M264" { print NR; exit }' "$FILE")
  if [ -z "$stop" ] || [ -z "$on" ] || [ "$stop" -gt "$on" ]; then
    ui_fail "the sub-spindle does not stop before its C axis goes on"
  fi
  [ "$(tail -n 2 "$FILE" | tr '\n' '|')" = "M30|%|" ] || ui_fail "the program does not end with M30 and the tape mark"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
