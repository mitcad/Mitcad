#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp app/OcctViewer.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverSketch.cpp core/sketch tools/cli
# check-all sources: tools/ui-scripts/sketch-trim-drag.mitcad-ui tools/ui-scripts/sketch-trim-drag.json
# check-all sources: tools/ui-scripts/sketch-trim-drag-verify.py
# Trim's drag (mitcad#2524, GitHub mitcad/Mitcad#657), the shared input
# script tools/ui-scripts/sketch-trim-drag.mitcad-ui (also run by
# tools/ui-windows-sketch-trim-drag-test.ps1 and
# tools/ui-macos-sketch-trim-drag-test.sh): a drag that starts on a curve
# trims nothing while the button is held and Esc drops it; a drag from
# empty space shows and then trims three pieces back to their crossings as
# one undo step, which one Undo takes back; a plain click trims one piece;
# with Trim ended a left drag selects with a window. The captured sketches
# are checked by tools/ui-scripts/sketch-trim-drag-verify.py.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-trim-drag-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-trim-drag.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-trim-drag.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-trim-drag.json" --save "$WORK/trim-drag.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/trim-drag.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch Trim drag driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Trim drag cancelled: 2 pieces not trimmed' 'Esc drops a drag that started on a curve'
ui_expect_log 'Trim drag trimmed 3 of 3 pieces as one step' 'a drag from empty space trims three pieces'
ui_expect_log 'TestDriver: done' 'the drag, its Esc, its Undo, a click and a window selection after Trim'
python3 -I "$here/ui-scripts/sketch-trim-drag-verify.py" "$WORK" ||
  ui_fail 'the captured sketches do not show the trims as expected'
ui_finish 'UI sketch Trim drag'
