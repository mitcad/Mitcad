#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SurfaceCommands.cpp app/framework/OpeningEdges.cpp app/OcctViewer.cpp tools/cli
# check-all sources: tools/ui-scripts/patch-opening.mitcad-ui tools/ui-scripts/patch-opening.json
# Patch picks a surface body's free edge as its opening and closes it
# (mitcad#1927): the shared input script
# tools/ui-scripts/patch-opening.mitcad-ui of
# tools/ui-patch-opening-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-patch-opening-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/patch-opening.json" --save "$MITCAD_TEST_WORK/patch-opening.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/patch-opening.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app patch-opening --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/patch-opening.mitcad"
ui_expect_log '^Patch Profiles or Edges: 1 edge' 'a click on a free edge takes the whole opening'
ui_expect_log '^Added patch of 1 items, stitched' 'OK patches the opening and stitches it on'
ui_expect_log '^TestDriver: done' 'the box is a closed solid again'
ui_expect_no_log 'TestDriver: FAIL' 'no failed Patch opening assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Patch picks an opening by its free edge and closes it'
