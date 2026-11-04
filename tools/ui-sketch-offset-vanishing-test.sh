#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# Sketch Offset where corner arcs vanish (mitcad#1234), the shared input
# script tools/ui-scripts/sketch-offset-vanishing.mitcad-ui (also run by
# tools/ui-windows-sketch-offset-vanishing-test.ps1): a hexagon with 2 mm
# corner arcs, Offset opened on a side with Chain Selection on previews at
# 5 mm and at 2 mm inward without an error, the note naming the corner
# arcs that vanish, and OK keeps the offset.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-offset-vanishing-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-offset-vanishing.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-offset-vanishing.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-offset-vanishing.json" --save "$WORK/offset.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/offset.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch Offset driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Offset previews and keeps a hexagon whose corner arcs vanish'
ui_finish 'UI sketch Offset where corner arcs vanish'
