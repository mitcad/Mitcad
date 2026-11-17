#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/Command.hpp app/framework/CommandPanel.cpp app/MainWindowSelection.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-pattern-honeycomb.mitcad-ui tools/ui-scripts/sketch-pattern-honeycomb.json
# The sketch Rectangular Pattern's Direction 2 (mitcad#1969, GitHub
# mitcad/Mitcad#406), the shared input script
# tools/ui-scripts/sketch-pattern-honeycomb.mitcad-ui (also run by
# tools/ui-windows-sketch-pattern-honeycomb-test.ps1 and
# tools/ui-macos-sketch-pattern-honeycomb-test.sh): a line at 60 degrees
# picked as Direction 2 runs the second row of copies along it and turns the
# handle's second arrow; Edit Pattern keeps it, and At Right Angles turns
# the row back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-pattern-honeycomb-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-pattern-honeycomb.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-pattern-honeycomb.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-pattern-honeycomb.json" --save "$WORK/pattern.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/pattern.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch pattern honeycomb driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the second row along the picked 60 degree line, kept by Edit Pattern'
ui_finish "UI sketch Rectangular Pattern's Direction 2 at 60 degrees"
