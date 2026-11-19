#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs
# The Sketch Palette's Construction (X) box (mitcad#2040), with the driver
# script shared with Linux and Windows.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo 'usage: ui-macos-sketch-construction-box-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-construction-box.json" --save "$MITCAD_TEST_WORK/lines.mitcad" || exit 1
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-construction-box.mitcad-ui" > "$MITCAD_TEST_WORK/workflow.mitcad-ui"
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/workflow.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sketch-construction-box --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/lines.mitcad"
ui_expect_log '^TestDriver: done' 'the box converts the selection as X does and shows its state'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: sketch construction box'
