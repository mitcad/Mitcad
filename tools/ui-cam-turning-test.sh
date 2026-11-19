#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/resources/layout/default.json
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-turning.mitcad-ui tools/ui-scripts/cam-turning.json
# CAM turning in the CNC group (mitcad#1869, app/COMMANDS.md "Turning in
# the CNC group"), the shared input script
# tools/ui-scripts/cam-turning.mitcad-ui (also run by
# tools/ui-windows-cam-turning-test.ps1 and tools/ui-macos-cam-turning-test.sh)
# on a block turned on a lathe (tools/ui-scripts/cam-turning.json): a lathe
# setup; Turn Facing, Turn Roughing, Turn Finishing, Threading and Parting
# Off from their panels; Generate (every operation up to date and drawn);
# Simulate (the turned stock drawn as its section revolved, its summary,
# nothing cut into the part); Post
# Process… to the Fanuc turning dialect: the file's program number, its
# modes (millimetres, feeds per revolution), the constant surface speed
# under its limit, the cycles G71, G70 and G76 and the threading's constant
# speed, X written as a diameter, M30 and the tape mark at the end.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cam-turning-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-cam-turning.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/cam-turning.mitcad-ui
export MITCAD_CAM_LIBRARY_DIR=$WORK/cam-library
"$CLI" run "$here/ui-scripts/cam-turning.json" --save "$WORK/cam-turning.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/cam-turning.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "CAM turning driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "a lathe setup, five turning operations, generate, simulate and post"

# The Fanuc turning program.
FILE=$WORK/setup1.nc
[ -s "$FILE" ] || ui_fail "no program written"
[ "$(sed -n 1p "$FILE")" = "%" ] || ui_fail "the program does not start with the tape mark: $(head -2 "$FILE")"
[ "$(sed -n 2p "$FILE")" = "O1000 (SETUP 1)" ] || ui_fail "the program's number is not first: $(sed -n 2p "$FILE")"
grep -qx 'G21 G18 G40 G99' "$FILE" || ui_fail "no millimetres and feeds per revolution in the header"
grep -qx 'G50 S3000' "$FILE" || ui_fail "no spindle limit before the constant surface speed"
grep -qx 'G96 S200 M3' "$FILE" || ui_fail "no constant surface speed"
grep -qE '^G71 U[0-9.]+ R[0-9.]+$' "$FILE" || ui_fail "no roughing cycle"
grep -qE '^G70 P[0-9]+ Q[0-9]+$' "$FILE" || ui_fail "no finishing cycle"
grep -qE '^G97 S[1-9][0-9]* M3$' "$FILE" || ui_fail "the thread is not cut at a constant speed"
grep -qE '^G76 P010060 ' "$FILE" || ui_fail "no threading cycle"
# X as a diameter: the bar's 44 mm plus the clearance outside it.
grep -qE '^G0 X48[.] Z' "$FILE" || ui_fail "X is not written as a diameter"
for name in "TURN FACING 1" "TURN ROUGHING 1" "TURN FINISHING 1" "THREADING 1" "PARTING OFF 1"; do
  grep -qxF "($name)" "$FILE" || ui_fail "no $name in the program"
done
[ "$(tail -n 2 "$FILE" | tr '\n' '|')" = "M30|%|" ] || ui_fail "the program does not end with M30 and the tape mark"
echo "ok   the Fanuc turning program: number, modes, G50/G96/G97, G71, G70, G76, diameters"
ui_finish "UI CAM turning: the lathe operations in the CNC group"
