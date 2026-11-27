#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp core/model/src/features/isoparametric_curve.rs geometry/src/iso_curve.cpp app/framework/ModelShapes.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/isoparametric-curve.mitcad-ui tools/ui-scripts/isoparametric-curve.json
# Isoparametric Curve (mitcad#2522): the shared input script
# tools/ui-scripts/isoparametric-curve.mitcad-ui of
# tools/ui-isoparametric-curve-test.sh in the Qt frontend. The u curve
# through the clicked side face, three v curves evenly spaced, the cylinder
# made taller and the first curve following it, the first curve reopened.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-isoparametric-curve-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/isoparametric-curve.json" --save "$MITCAD_TEST_WORK/isoparametric-curve.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/isoparametric-curve.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app isoparametric-curve --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/isoparametric-curve.mitcad"
ui_expect_log 'Added isoparametric curve, U direction, through the picked point' 'the u curve was added through the click'
ui_expect_log 'Added isoparametric curve, V direction, 3 evenly spaced' 'three v curves were added'
ui_expect_log '^TestDriver: done' 'Isoparametric Curve (mitcad#2522): every step of the script'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Isoparametric Curve (mitcad#2522)'
