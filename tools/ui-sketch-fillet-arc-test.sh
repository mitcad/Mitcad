#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# Sketch Fillet at a corner of a line and an arc (mitcad#1192), the shared
# input script tools/ui-scripts/sketch-fillet-arc.mitcad-ui (also run by
# tools/ui-windows-sketch-fillet-arc-test.ps1): a half disc's corner point
# where its line and its arc end (two points held together, as a Trim
# leaves them) selected, Sketch Fillet takes it,
# previews and rounds the corner with the default radius 2; the saved
# sketch's one region is the half disc less the rounded-off corner.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-fillet-arc-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-fillet-arc.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-fillet-arc.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-fillet-arc.json" --save "$WORK/fillet.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/fillet.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Sketch Fillet driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Sketch Fillet at the corner point of a line and an arc'
"$CLI" run "$here/ui-scripts/sketch-fillet-arc-check.json" --open "$WORK/fillet.mitcad" ||
  ui_fail 'the saved sketch has no fillet of r 2 at the corner of the half disc'
ui_finish 'UI Sketch Fillet of a line and an arc'
