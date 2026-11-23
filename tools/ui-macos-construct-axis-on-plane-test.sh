#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/construction core/model/src/features/geom_ref.rs tools/cli
# check-all sources: tools/ui-scripts/construct-axis-on-plane.mitcad-ui tools/ui-scripts/construct-axis-on-plane.json
# Axis Perpendicular at Point on a plane (mitcad#2282) in the Qt frontend on
# macOS, as tools/ui-construct-axis-on-plane-test.sh on Linux: the shared
# steps tools/ui-scripts/construct-axis-on-plane.mitcad-ui on a design that
# mitcad-cli makes (tools/ui-scripts/construct-axis-on-plane.json): its Face
# field takes a construction plane and the origin plane XY, and the axis
# goes through the point's projection onto the plane, along its normal.
# Usage: tools/ui-macos-construct-axis-on-plane-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-construct-axis-on-plane-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/axis-on-plane.mitcad
"$cli" run "$here/ui-scripts/construct-axis-on-plane.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run construct-axis-on-plane.json' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/construct-axis-on-plane.mitcad-ui MITCAD_TEST_STEP_MS=50 MITCAD_LOG_PICKS=1 \
  ui_run_app construct-axis-on-plane --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$design"
ui_expect_log '^Added axis Axis1: origin [(]20, 0, 10[)], direction [(]0, 0, 1[)]$' \
  "the axis on a construction plane goes through the point's projection along its normal"
ui_expect_log '^Added axis Axis2: origin [(]20, 0, 0[)], direction [(]0, 0, 1[)]$' 'the axis on the origin plane XY'
ui_expect_log '^TestDriver: done' 'both axes made on planes'
ui_expect_no_log 'TestDriver: FAIL' 'no failed axis on plane assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt construct axis on plane'
