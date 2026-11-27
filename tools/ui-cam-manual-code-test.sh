#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam/CamManualCode.cpp app/cam/CamManualCode.hpp app/cam/MainWindowCam.cpp
# check-all sources: app/browser/BrowserController.cpp app/browser/BrowserPanel.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: core/cam/src/turning core/model/src/cam tools/cli
# check-all sources: tools/ui-scripts/cam-manual-code.mitcad-ui tools/ui-scripts/cam-turning.json
# A lathe setup's manual G-code in the setup panel and the Browser
# (mitcad#2506, mitcad#2532, app/COMMANDS.md "Turning in the CNC group"),
# the input script tools/ui-scripts/cam-manual-code.mitcad-ui on a block
# turned on a lathe (tools/ui-scripts/cam-turning.json): three blocks added
# in New Setup, moved up and down with the rows' ▲ and ▼ and one removed
# with ✕; a block dragged in the Browser after an operation and one taken
# out with the Delete key (one undo step each); Post Process… to the Fanuc
# turning dialect: the blocks in the program in the Browser's order, the
# removed one nowhere.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cam-manual-code-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-cam-manual-code.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/cam-manual-code.mitcad-ui
export MITCAD_CAM_LIBRARY_DIR=$WORK/cam-library
"$CLI" run "$here/ui-scripts/cam-turning.json" --save "$WORK/cam-manual-code.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/cam-manual-code.mitcad"
for _ in $(seq 1 1000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "CAM manual G-code driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "manual G-code rows added, moved and removed; a block dragged and deleted in the Browser; post"

# The program: Gamma at the start, Alpha after Turn Facing 1, Beta nowhere.
FILE=$WORK/manual.nc
[ -s "$FILE" ] || ui_fail "no program written"
line_of() { grep -n -F -- "$1" "$FILE" | head -n 1 | cut -d: -f1; }
gamma=$(line_of '(MANUAL CODE BEGIN: GAMMA')
facing=$(line_of '(TURN FACING 1)')
alpha=$(line_of '(MANUAL CODE BEGIN: ALPHA')
roughing=$(line_of '(TURN ROUGHING 1)')
if [ -z "$gamma" ] || [ -z "$facing" ] || [ -z "$alpha" ] || [ -z "$roughing" ]; then
  ui_fail "a block or an operation is missing: Gamma '$gamma', Turn Facing 1 '$facing', Alpha '$alpha', Turn Roughing 1 '$roughing'"
elif ! [ "$gamma" -lt "$facing" ] || ! [ "$facing" -lt "$alpha" ] || ! [ "$alpha" -lt "$roughing" ]; then
  ui_fail "not in the order Gamma, Turn Facing 1, Alpha, Turn Roughing 1: lines $gamma, $facing, $alpha, $roughing"
fi
[ "$(grep -c -F '(MANUAL CODE BEGIN:' "$FILE")" = 2 ] || ui_fail "not two blocks of manual G-code in the program"
grep -q 'BETA' "$FILE" && ui_fail "the removed block Beta is in the program"
grep -qx 'G4 X0.3' "$FILE" || ui_fail "Gamma's line is not written as it is"
grep -qx 'G4 X0.1' "$FILE" || ui_fail "Alpha's line is not written as it is"
grep -qx 'G4 X0.2' "$FILE" && ui_fail "Beta's line is in the program"
echo "ok   the program: Gamma, Turn Facing 1, Alpha, Turn Roughing 1; no Beta"
ui_finish "UI CAM manual G-code: the setup panel's rows and the Browser's drag and Delete"
