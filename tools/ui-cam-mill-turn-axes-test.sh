#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/resources/layout/default.json
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-mill-turn-axes.mitcad-ui tools/ui-scripts/cam-mill-turn-axes.json
# CAM mill-turn with a Y axis, a B head and a sub-spindle in the CNC group
# (mitcad#2319, app/COMMANDS.md "Live tools on a lathe"), the shared input
# script tools/ui-scripts/cam-mill-turn-axes.mitcad-ui (also run by
# tools/ui-windows-cam-mill-turn-axes-test.ps1 and
# tools/ui-macos-cam-mill-turn-axes-test.sh) on a block turned on a lathe
# with every axis (tools/ui-scripts/cam-mill-turn-axes.json): a lathe setup
# handing over to the sub-spindle; Turn Facing from its panel, a flat with
# the Y axis, a hole on a plane tilted by the B head, a sketch wrapped onto
# the diameter, the part-off and the back on the sub-spindle; Generate;
# Simulate (the stock cut in 3D); Post Process… to the Fanuc turning
# dialect: G19 with the C axis clamped for the flat, the tilted working
# plane (G68.2, G53.1, G69), cylindrical interpolation on the 15 mm
# cylinder the sketch wraps on, the spindles synchronised for the
# hand-over, the sub-spindle's chuck, work offset and spindle, M30 and the
# tape mark at the end.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cam-mill-turn-axes-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-cam-mill-turn-axes.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/cam-mill-turn-axes.mitcad-ui
export MITCAD_CAM_LIBRARY_DIR=$WORK/cam-library
"$CLI" run "$here/ui-scripts/cam-mill-turn-axes.json" --save "$WORK/cam-mill-turn-axes.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/cam-mill-turn-axes.mitcad"
for _ in $(seq 1 2000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "CAM mill-turn axes driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "a lathe setup with a hand-over, Y, B and wrapped live tools, generate, simulate and post"

# The Fanuc program with the Y axis, the B head and the sub-spindle.
FILE=$WORK/setup1.nc
[ -s "$FILE" ] || ui_fail "no program written"
[ "$(sed -n 1p "$FILE")" = "%" ] || ui_fail "the program does not start with the tape mark: $(head -2 "$FILE")"
for line in M154 G98 'G0 C0.' M14 G19 G18 'G68.2 X0. Y0. Z0. I90. J45. K-90.' G53.1 M107 G69 'G7.1 C15.' 'G7.1 C0.' \
  M213 M79 M78 M212 G55 M155 G99; do
  grep -qxF "$line" "$FILE" || ui_fail "no '$line' in the program"
done
grep -qE '^G83 X-?[0-9.]+ Y-?[0-9.]+ Z-?[0-9.]+ R-?[0-9.]+( Q[0-9]+)? F[0-9.]+$' "$FILE" || ui_fail "no drilling cycle on the tilted plane"
grep -qE '^G1 A-?[0-9.]+ F[0-9.]+$' "$FILE" || ui_fail "the sub-spindle does not come up to the part"
grep -qE '^G96 S[0-9]+ M203$' "$FILE" || ui_fail "the sub-spindle does not turn the part's back"
for name in "TURN FACING 1" "FACING 1" "DRILL 1" "CONTOUR 1" "PARTING OFF 1" "TURN FACING 2" "HAND-OVER TO THE SUB-SPINDLE"; do
  grep -qxF "($name)" "$FILE" || ui_fail "no $name in the program"
done
[ "$(tail -n 2 "$FILE" | tr '\n' '|')" = "M30|%|" ] || ui_fail "the program does not end with M30 and the tape mark"
echo "ok   the Fanuc program with Y, B and the sub-spindle: G19, G68.2, G7.1, the hand-over, G55, M203"
ui_finish "UI CAM mill-turn axes: Y, B, wrapped sketches, the sub-spindle and the 3D stock"
