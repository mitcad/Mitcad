#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp app/OcctViewer.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverSketch.cpp core/sketch tools/cli
# check-all sources: tools/ui-scripts/sketch-trim-drag.mitcad-ui tools/ui-scripts/sketch-trim-drag.json
# Trim's drag (mitcad#2524) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/sketch-trim-drag.mitcad-ui on the design mitcad-cli
# makes (tools/ui-scripts/sketch-trim-drag.json): a drag that starts on a
# curve trims nothing while the button is held and Esc drops it, a drag
# from empty space trims three pieces as one undo step that one Undo takes
# back, a plain click trims one piece, and with Trim ended a left drag
# selects with a window. As tools/ui-sketch-trim-drag-test.sh (Linux) and
# tools/ui-windows-sketch-trim-drag-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-trim-drag-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-trim-drag}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-trim-drag.json" --save "$MITCAD_TEST_WORK/trim-drag.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-trim-drag.mitcad-ui" > "$MITCAD_TEST_WORK/workflow.mitcad-ui"
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/workflow.mitcad-ui \
  ui_run_app sketch-trim-drag --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/trim-drag.mitcad"
ui_expect_log 'Trim drag trimmed 3 of 3 pieces as one step' 'a drag from empty space trims three pieces'
ui_expect_log 'TestDriver: done' 'the drag, its Esc, its Undo, a click and a window selection after Trim'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch Trim drag; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch Trim drag; evidence in $UI_OUT"
