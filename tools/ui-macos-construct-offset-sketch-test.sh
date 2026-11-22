#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/commands/CommandSupport.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/construct-offset-sketch.mitcad-ui
# Offset Plane takes a sketch (mitcad#2167) on macOS, through the test
# driver (tools/ui-scripts/construct-offset-sketch.mitcad-ui, the same script
# as the Linux and Windows tests'): the sketch picked in the Browser and its
# profile clicked in the view are each taken as the sketch's plane, and OK
# makes the plane 15 mm above it.
# Usage: tools/ui-macos-construct-offset-sketch-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-construct-offset-sketch}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/construct-offset-sketch.json" --save "$UI_OUT/files/construct-offset-sketch.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/construct-offset-sketch.mitcad-ui \
  ui_run_app construct-offset-sketch --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/construct-offset-sketch.mitcad"
ui_expect_log "Construction plane: the sketch of F1 taken as its sketch's plane" \
  'the sketch picked in the Browser was taken'
ui_expect_log "Construction plane: a profile of F1 taken as its sketch's plane" \
  'the profile clicked in the view was taken'
ui_expect_log 'Added plane Plane[0-9]+: origin [(][-0-9.e]+, [-0-9.e]+, 15[)], normal [(]0, 0, 1[)]' \
  'OK made the plane 15 mm above the sketch'
ui_expect_log 'TestDriver: done' 'Offset Plane on a sketch'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
