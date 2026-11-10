#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/Command.hpp app/MainWindowSelection.cpp app/framework/TestDriver.cpp
# check-all sources: core/sketch core/model-api/src/sketch.rs tools/cli
# Sketch Move/Copy (mitcad#1563, GitHub mitcad/Mitcad#212), the shared
# input script tools/ui-scripts/sketch-move-p2p.mitcad-ui (also run by
# tools/ui-windows-sketch-move-p2p-test.ps1): Point to Point moves a
# rectangle from its corner to a circle's center, both picked with the
# sketch's snaps; Translate's handle dragged onto a point sets X and Y
# Distance; Rotate without a Pivot Point turns a horizontal line 90
# degrees about its middle.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-move-p2p-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-move-p2p.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-move-p2p.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-move-p2p.json" --save "$WORK/move.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/move.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch Move/Copy driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Point to Point, the handle and Rotate about the middle'
ui_finish 'UI sketch Move/Copy Point to Point, handle and pivot'
