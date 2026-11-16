#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-move-text.mitcad-ui tools/ui-scripts/sketch-move-text.json
# check-all sources: tools/ui-scripts/sketch-move-text-check.json
# A sketch text moved after it is made (mitcad#1911, GitHub
# mitcad/Mitcad#369), the shared input script
# tools/ui-scripts/sketch-move-text.mitcad-ui (also run by
# app.ui-windows-sketch-move-text and app.ui-macos-sketch-move-text): a
# text placed with the Text tool, moved 10 mm with Move/Copy, dragged onto a
# point and placed by Edit Text's Position, its anchor read back in Edit
# Text each time; the saved sketch has the text where the Position put it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-move-text-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-move-text.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-move-text.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-move-text.json" --save "$WORK/text.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/text.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch text move driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'a text moved with Move/Copy, dragged and placed by its Position'
"$CLI" run "$here/ui-scripts/sketch-move-text-check.json" --open "$WORK/text.mitcad" ||
  ui_fail 'the saved text is not where Edit Text placed it'
ui_finish 'UI sketch text moved'
