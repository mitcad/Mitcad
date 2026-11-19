#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-text-wrap.mitcad-ui tools/ui-scripts/sketch-text-wrap.json
# check-all sources: tools/ui-scripts/sketch-text-wrap-check.json
# A sketch text drawn in a frame wraps to the frame's width (mitcad#2029,
# GitHub mitcad/Mitcad#453), the shared input script
# tools/ui-scripts/sketch-text-wrap.mitcad-ui (also run by
# app.ui-windows-sketch-text-wrap and app.ui-macos-sketch-text-wrap): a
# frame dragged with the Text tool and a long text typed, its lines within
# the frame; Edit Text's Height, a frame corner dragged, Wrap in Frame off
# and on, and a line break typed in Edit Text's Text box; the saved sketch
# has the text in two lines, wrapped.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-text-wrap-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-text-wrap.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-text-wrap.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-text-wrap.json" --save "$WORK/text.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/text.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch text wrap driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'a text wrapped in its frame, rewrapped and typed in two lines'
"$CLI" run "$here/ui-scripts/sketch-text-wrap-check.json" --open "$WORK/text.mitcad" ||
  ui_fail 'the saved text is not wrapped in two lines'
ui_finish 'UI sketch text wrapped in its frame'
