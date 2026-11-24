#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/resources/layout/default.json
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-mill-turn.mitcad-ui tools/ui-scripts/cam-mill-turn.json
# CAM mill-turn in the CNC group (mitcad#2067, app/COMMANDS.md "Live tools
# on a lathe"), the shared input script
# tools/ui-scripts/cam-mill-turn.mitcad-ui (also run by
# tools/ui-windows-cam-mill-turn-test.ps1 and
# tools/ui-macos-cam-mill-turn-test.sh) on a block turned on a lathe with a
# C axis and live tools (tools/ui-scripts/cam-mill-turn.json): a lathe
# setup; Turn Facing and a Drill on the face from their panels, a pocket on
# the face and a cross hole on the diameter; Generate; Simulate (the turned
# section and the live tools' cuts in words); Post Process… to the Fanuc
# turning dialect: the C axis switched on with the lathe's codes and off
# again, feeds per minute for the live tools, the C axis indexed and
# clamped for the hole on the face, polar interpolation for the pocket,
# the side drilling cycle for the cross hole, M30 and the tape mark at the
# end.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cam-mill-turn-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-cam-mill-turn.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/cam-mill-turn.mitcad-ui
export MITCAD_CAM_LIBRARY_DIR=$WORK/cam-library
"$CLI" run "$here/ui-scripts/cam-mill-turn.json" --save "$WORK/cam-mill-turn.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/cam-mill-turn.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "CAM mill-turn driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "a lathe setup, turning and live tool operations, generate, simulate and post"

# The Fanuc program with the C axis.
FILE=$WORK/setup1.nc
[ -s "$FILE" ] || ui_fail "no program written"
[ "$(sed -n 1p "$FILE")" = "%" ] || ui_fail "the program does not start with the tape mark: $(head -2 "$FILE")"
grep -qx 'G21 G18 G40 G99' "$FILE" || ui_fail "no millimetres and feeds per revolution in the header"
grep -qx 'G96 S200 M3' "$FILE" || ui_fail "no turning before the live tools"
for line in M5 M154 'G28 H0.' G98 'M133 P2000' M14 M15 G17 G12.1 G13.1 M135 M155 G99; do
  grep -qxF "$line" "$FILE" || ui_fail "no '$line' in the program"
done
grep -qE '^G0 X[0-9.]+ C-?[0-9.]+$' "$FILE" || ui_fail "the C axis is not indexed for the hole on the face"
grep -qE '^G87 Z-5[.] C0[.] X30[.] R-?[0-9.]+ F[0-9.]+$' "$FILE" || ui_fail "no side drilling cycle for the cross hole"
for name in "TURN FACING 1" "DRILL 1" "POCKET 1" "DRILL 2"; do
  grep -qxF "($name)" "$FILE" || ui_fail "no $name in the program"
done
[ "$(tail -n 2 "$FILE" | tr '\n' '|')" = "M30|%|" ] || ui_fail "the program does not end with M30 and the tape mark"
echo "ok   the Fanuc program with the C axis: M154/M155, G98/G99, the clamp, G12.1, G87"
ui_finish "UI CAM mill-turn: live tools on a lathe's face and diameter"
