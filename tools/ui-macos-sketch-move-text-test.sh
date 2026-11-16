#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs
# A sketch text moved after it is made (mitcad#1911) on macOS in Qt, as
# tools/ui-sketch-move-text-test.sh on Linux: the shared input script
# tools/ui-scripts/sketch-move-text.mitcad-ui, a text placed with the Text
# tool, moved 10 mm with Move/Copy, dragged onto a point and placed by Edit
# Text's Position, its anchor read back in Edit Text each time; the saved
# sketch has the text where the Position put it.
# Usage: tools/ui-macos-sketch-move-text-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sketch-move-text-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-move-text.json" --save "$MITCAD_TEST_WORK/text.mitcad" || exit 1
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" "$here/ui-scripts/sketch-move-text.mitcad-ui" \
  > "$MITCAD_TEST_WORK/workflow.mitcad-ui"
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/workflow.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sketch-move-text --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/text.mitcad"
ui_expect_log '^TestDriver: done' 'a text moved with Move/Copy, dragged and placed by its Position'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
"$cli" run "$here/ui-scripts/sketch-move-text-check.json" --open "$MITCAD_TEST_WORK/text.mitcad" || exit 1
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt sketch text moved'
