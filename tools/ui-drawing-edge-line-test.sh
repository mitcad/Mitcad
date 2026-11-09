#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawing_lines.rs core/model-api/src/drawing_annotations.rs
# check-all sources: core/model-drawing/src/compose.rs core/model-drawing/src/sheet.rs
# check-all sources: tools/ui-scripts/drawing-edge-line.mitcad-ui tools/ui-scripts/drawing-edge-line.json
# Edit Line on a projected edge (mitcad#1546) through the real UI, the
# shared input script tools/ui-scripts/drawing-edge-line.mitcad-ui (also
# run by tools/ui-windows-drawing-edge-line-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/drawing-edge-line.json: a step whose
# edge runs through the middle of its top view): a right click on the
# edge offers Edit Line…, whose Thick and Long Dash Double Dot make that
# edge's line wide and long dash double dotted in the view (the sheet's
# pen phantom_thick), and Reset to View Style takes the choice away.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-edge-line-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-edge-line.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/drawing-edge-line.mitcad-ui
"$cli" run "$here/ui-scripts/drawing-edge-line.json" --save "$work/step.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/step.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Edit Line driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Drawing line dialog opened: edge ' 'Edit Line opened on the edge under the pointer'
ui_expect_log 'Drawing view pens: ' 'the sheet logged its pens'
ui_expect_log 'TestDriver: done' 'Edit Line set the edge line width and pattern and reset them'
ui_finish 'UI drawing Edit Line on a projected edge'
