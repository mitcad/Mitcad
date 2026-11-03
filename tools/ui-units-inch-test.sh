#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/LengthUnits.cpp app/framework/CommandSession.cpp app/browser/BrowserPanel.cpp
# check-all sources: app/commands/InspectCommands.cpp app/commands/ExtrudeCommands.cpp tools/cli
# check-all sources: app/view/GridSpacing.cpp app/view/ViewController.cpp app/LayoutGrid.cpp app/OcctViewer.cpp tools/ui-scripts/units-inch.mitcad-ui
# Lengths in the design's unit (mitcad#1165) through the real UI, on a
# design in inches that mitcad-cli makes (tools/cli/tests/units_inch.json):
# the Browser's sheet-rules row shows 0.05 in and R 0.1 in, Extrude opens
# with its default 10 mm as 0.5 in, Measure shows a top's area as 2 in² and
# two parallel tops 1 in apart; the layout grid in inches (mitcad#1210):
# Grid Settings shows the fixed spacing in inches, the automatic grid steps
# through the inch series when zooming, and after the design set to mm the
# grid is metric again with the fixed spacing kept. The steps are
# tools/ui-scripts/units-inch.mitcad-ui, which
# tools/ui-windows-units-inch-test.ps1 runs on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-units-inch-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-units-inch.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/units-inch.mitcad-ui
"$cli" run "$here/cli/tests/units_inch.json" --save "$work/inch.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/inch.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'units in inches driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the sheet-rules row, Extrude, Measure and the layout grid in inches'
ui_finish 'UI lengths in the design unit'
