#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  echo 'usage: ui-macos-sketch-polygon-dimensions-test.sh <app> <cli> <output> [qt|native]' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
frontend=${4:-qt}
case "$frontend" in qt|native) ;; *) echo 'expected qt or native frontend' >&2; exit 2 ;; esac
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-polygon-dimensions.json" --save "$MITCAD_TEST_WORK/polygons.mitcad" || exit 1
if [ "$frontend" = native ]; then
  enter='native-activate native.history.F1'
else
  enter="dblclick-logged '^Timeline Sketch1 at '"
fi
sed "s/^ENTER_SKETCH$/$enter/" "$here/ui-scripts/sketch-polygon-dimensions.mitcad-ui" \
  > "$MITCAD_TEST_WORK/workflow.mitcad-ui"
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/workflow.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app polygon-dimensions --ui="$frontend" --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/polygons.mitcad"
ui_expect_log '^TestDriver: done' "$frontend: typed sizes, sides-only input and single-step Undo/Redo"
ui_expect_no_log 'TestDriver: FAIL' "$frontend: no failed drawing assertions"
"$cli" run "$here/ui-scripts/sketch-polygon-dimensions-check.json" --open "$MITCAD_TEST_WORK/polygons.mitcad" || exit 1
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: $frontend polygon typed dimensions"
