#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/CellGridCommands.cpp app/commands/CreateCommands.cpp core/model/src/features/cell_grid.rs core/model/src/features/cell_grid app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/cell-grid.mitcad-ui tools/ui-scripts/cell-grid.json
# Cell Grid (mitcad#2238): the shared input script tools/ui-scripts/cell-grid.mitcad-ui of
# tools/ui-cell-grid-test.sh in the Qt frontend. A rectangle with a circle inside sketched on a plate's top face, Cell Grid of hexagons on the rectangle's region, then edited to circles, with the cell counts and the volumes the script works out.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-cell-grid-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/cell-grid.json" --save "$MITCAD_TEST_WORK/cell-grid.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/cell-grid.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app cell-grid --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/cell-grid.mitcad"
ui_expect_log 'Preview Cell Grid: ok, remark: 34 cells, open area 1884.47 mm' 'the hexagons counted'
ui_expect_log 'Added cell grid [(]hexagon, whole edge cells[)] of 1 profile' 'the cell grid was added'
ui_expect_log 'Preview Cell Grid: ok, remark: 34 cells, open area 1709.03 mm' 'the circles counted'
ui_expect_log '^TestDriver: done' 'Cell Grid (mitcad#2238): every step of the script'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Cell Grid (mitcad#2238)'
