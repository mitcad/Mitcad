#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# A sketch text drawn in a frame wraps to the frame's width (mitcad#2029)
# on macOS in Qt, as tools/ui-sketch-text-wrap-test.sh on Linux: the shared
# input script tools/ui-scripts/sketch-text-wrap.mitcad-ui, a frame dragged
# with the Text tool and a long text typed, its lines within the frame;
# Edit Text's Height, a frame corner dragged, Wrap in Frame off and on, and
# a line break typed in Edit Text's Text box; the saved sketch has the text
# in two lines, wrapped.
# Usage: tools/ui-macos-sketch-text-wrap-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sketch-text-wrap-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-text-wrap.json" --save "$MITCAD_TEST_WORK/text.mitcad" || exit 1
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" "$here/ui-scripts/sketch-text-wrap.mitcad-ui" \
  > "$MITCAD_TEST_WORK/workflow.mitcad-ui"
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/workflow.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sketch-text-wrap --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/text.mitcad"
ui_expect_log '^TestDriver: done' 'a text wrapped in its frame, rewrapped and typed in two lines'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
"$cli" run "$here/ui-scripts/sketch-text-wrap-check.json" --open "$MITCAD_TEST_WORK/text.mitcad" || exit 1
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt sketch text wrapped in its frame'
