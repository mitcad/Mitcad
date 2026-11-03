#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs
# A Center to Center Slot's typed length and width become its dimensions
# (mitcad#1225), through the real UI: the shared input script
# tools/ui-scripts/sketch-slot-dimensions.mitcad-ui (also run by
# tools/ui-windows-sketch-slot-dimensions-test.ps1) on an empty sketch that
# mitcad-cli makes; mitcad-cli then checks the saved design
# (tools/ui-scripts/sketch-slot-dimensions-check.json): the slot keeps only
# its place free, and dimensions that move it keep its length.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-slot-dimensions-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-slot-dimensions.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-slot-dimensions.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-slot-dimensions.json" --save "$WORK/slots.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/slots.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'slot dimensions driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "the slot's typed length and width are its dimensions, in one undo step"
"$CLI" run "$here/ui-scripts/sketch-slot-dimensions-check.json" --open "$WORK/slots.mitcad" || exit 1
ui_finish 'UI slot typed dimensions'
