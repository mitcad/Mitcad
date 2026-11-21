#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/MainWindowContextMenu.cpp app/framework/Command.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp core/model/src/features/construction
# check-all sources: core/model/src/features/coordinate_system.rs tools/cli
# check-all sources: tools/ui-scripts/construct-perpendicular-plane.mitcad-ui tools/ui-scripts/construct-perpendicular-plane.json
# Plane Perpendicular to Face at Point (mitcad#2111) in the Qt frontend on
# macOS, as tools/ui-construct-perpendicular-plane-test.sh on Linux: the
# shared steps tools/ui-scripts/construct-perpendicular-plane.mitcad-ui on a
# design that mitcad-cli makes
# (tools/ui-scripts/construct-perpendicular-plane.json): a cylinder's side
# and a sketch point selected, the view's context menu offers the plane in
# Construct; made level through the side's nearest point, turned 90 degrees
# through the cylinder's axis by its Angle, and a sketch on it lies in
# it, upright.
# Usage: tools/ui-macos-construct-perpendicular-plane-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-construct-perpendicular-plane-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/perpendicular.mitcad
"$cli" run "$here/ui-scripts/construct-perpendicular-plane.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run construct-perpendicular-plane.json' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/construct-perpendicular-plane.mitcad-ui MITCAD_TEST_STEP_MS=50 MITCAD_LOG_PICKS=1 \
  ui_run_app construct-perpendicular-plane --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$design"
ui_expect_log '^Command Plane Perpendicular to Face at Point started' \
  'the context menu offers the plane for a face and a point'
ui_expect_log '^Added plane Plane1: origin [(]10, 0, 10[)], normal [(]0, 0, -1[)]$' "level through the side's nearest point"
ui_expect_log '^Edited F3' 'the Angle changed'
ui_expect_log '^Sketch frame in the design: origin [(]10[.]000000, 0[.]000000, 10[.]000000[)], normal [(]0[.]000000, 1[.]000000, 0[.]000000[)], x axis [(]-1[.]000000, 0[.]000000, 0[.]000000[)]$' \
  'a sketch on the turned plane lies in it, upright'
ui_expect_log '^TestDriver: done' 'the perpendicular plane made, turned and sketched on'
ui_expect_no_log 'TestDriver: FAIL' 'no failed perpendicular plane assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt construct perpendicular plane'
