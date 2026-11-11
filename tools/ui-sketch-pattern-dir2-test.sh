#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/Command.hpp app/MainWindowSelection.cpp app/framework/TestDriver.cpp
# check-all sources: core/sketch core/model-api/src/sketch.rs tools/cli
# The sketch Rectangular Pattern's second direction (mitcad#1640, GitHub
# mitcad/Mitcad#243), the shared input script
# tools/ui-scripts/sketch-pattern-dir2.mitcad-ui (also run by
# tools/ui-windows-sketch-pattern-dir2-test.ps1): on a circle the pattern
# opens as a 3 x 2 grid (Quantity 2 of 2) with an arrow for each
# direction; dragging the second arrow changes Spacing 2; Quantity 2 of 1
# leaves that arrow faint.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-pattern-dir2-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-pattern-dir2.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-pattern-dir2.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-pattern-dir2.json" --save "$WORK/pattern.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/pattern.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch pattern driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'a 3 x 2 grid, the second arrow dragged and faint'
ui_finish "UI sketch Rectangular Pattern's second direction"
