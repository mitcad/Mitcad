#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/CellGridCommands.cpp app/commands/CreateCommands.cpp core/model/src/features/cell_grid.rs core/model/src/features/cell_grid app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/cell-grid.mitcad-ui tools/ui-scripts/cell-grid.json
# Cell Grid (mitcad#2238) through the real UI: the shared input script
# tools/ui-scripts/cell-grid.mitcad-ui (also run by
# tools/ui-windows-cell-grid-test.ps1 and tools/ui-macos-cell-grid-test.sh) on a
# design that mitcad-cli makes (tools/ui-scripts/cell-grid.json). A rectangle with a circle inside sketched on a plate's top face, Cell Grid of hexagons on the rectangle's region (the circle a keep-out), then edited to circles, with the cell counts and the volumes the script works out.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cell-grid-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-cell-grid.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/cell-grid.mitcad-ui
design=$work/cell-grid.mitcad
"$cli" run "$here/ui-scripts/cell-grid.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'cell-grid driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preview Cell Grid: ok, remark: 34 cells, open area 1884.47 mm' 'the hexagons counted'
ui_expect_log 'Added cell grid (hexagon, whole edge cells) of 1 profile' 'the cell grid was added'
ui_expect_log 'Preview Cell Grid: ok, remark: 34 cells, open area 1709.03 mm' 'the circles counted'
ui_expect_log 'TestDriver: done' 'Cell Grid (mitcad#2238): every step of the script'
ui_finish 'UI cell-grid'
