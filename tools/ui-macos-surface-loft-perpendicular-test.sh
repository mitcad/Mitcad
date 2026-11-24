#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SurfaceCommands.cpp app/commands/SweepCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/CommandPanel.cpp geometry/src/loft.cpp geometry/src/loft_skin.cpp
# check-all sources: core/model/src/features/loft.rs tools/cli
# check-all sources: tools/ui-scripts/surface-loft-perpendicular.mitcad-ui tools/ui-scripts/surface-loft-perpendicular.json
# Surface Loft's Start and End Condition (mitcad#2345): the shared input
# script tools/ui-scripts/surface-loft-perpendicular.mitcad-ui of
# tools/ui-surface-loft-perpendicular-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-surface-loft-perpendicular-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/surface-loft-perpendicular.json" --save "$MITCAD_TEST_WORK/loft.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/surface-loft-perpendicular.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app surface-loft-perpendicular --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/loft.mitcad"
ui_expect_log '^Added surface loft through 2 sections' 'the surface loft builds with Perpendicular to Face at both ends'
ui_expect_log '^TestDriver: done' 'the surface is one side face'
ui_expect_no_log 'TestDriver: FAIL' 'no failed surface loft perpendicular assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Surface Loft with Perpendicular to Face at both ends, one side face'
