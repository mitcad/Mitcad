#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/BusyMark.cpp app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/DocumentSession.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/panel-busy-waiting.mitcad-ui tools/ui-scripts/panel-message-top.json
# A panel's preview held while the model finishes another job shows the
# busy mark (mitcad#2118) on macOS, through the test driver
# (tools/ui-scripts/panel-busy-waiting.mitcad-ui, the same script as the
# Linux and Windows tests'): a cube's Extrude edited, Docked and then
# Floating, its Distance 7 mm, whose preview's display refinement the
# test's worker gate holds; a Distance of 9 mm typed meanwhile waits for it
# with the busy mark beside OK and Cancel shown, and once the job is let go
# its preview follows and the mark hides.
# Usage: tools/ui-macos-panel-busy-waiting-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-panel-busy-waiting}"
UI_TIMEOUT=180
mkdir -p "$UI_OUT/files"
design=$UI_OUT/files/panel-busy-waiting.mitcad
"$2" run "$here/ui-scripts/panel-message-top.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}

for layout in docked floating; do
  echo "--- $layout"
  MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 \
    MITCAD_TEST_INPUT=$here/ui-scripts/panel-busy-waiting.mitcad-ui \
    ui_run_app "panel-busy-waiting-$layout" --ui=qt --no-recovery --no-mcp --chrome=$layout --open "$design"
  ui_expect_log '^Preview Extrude: waits for the model' "$layout: the next preview waits for another job"
  ui_expect_log '^Panel Extrude busy mark shown after [0-9]+ ms: Waiting for the design to finish computing' \
    "$layout: the busy mark shown while it waits"
  ui_expect_log 'TestDriver: done' "$layout: a preview held by another job shows the busy mark"
  ui_expect_no_log 'TestDriver: FAIL' "$layout: no failed driver assertion"
done
[ "$UI_FAILURES" = 0 ] || exit 1
