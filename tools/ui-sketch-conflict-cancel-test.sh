#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/CommandSession.cpp app/commands/SketchCommands.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-conflict-cancel.mitcad-ui tools/ui-scripts/sketch-conflict-cancel.json
# Leaving a refused constraint tool puts the overlay back to the committed
# sketch (mitcad#1761, GitHub #318): the input script
# tools/ui-scripts/sketch-conflict-cancel.mitcad-ui on a design that
# mitcad-cli makes (tools/ui-scripts/sketch-conflict-cancel.json), two free
# points held 10 mm and 20 mm from a vertical line. Symmetry of the points
# about the line, and a Coincident of one on it, are refused for conflicting
# constraints and paint the distances that block them; Esc, another tool,
# the tool's next pick, a command panel (Mirror), Esc after a constraint
# applied to the selection and Finish Sketch each take the painting away
# ("Sketch conflict painted: none"), and the sketch's state line stays the
# committed one. The screenshot (sketch-conflict-cancel.png) shows the
# painting before Esc.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-conflict-cancel-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-conflict-cancel.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=${MITCAD_UI_LOG_DIR:-$WORK}
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-conflict-cancel.mitcad-ui
"$CLI" run "$here/ui-scripts/sketch-conflict-cancel.json" --save "$WORK/conflict-cancel.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating --open "$WORK/conflict-cancel.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch conflict cancel driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Constraint symmetric refused: ' 'Symmetry is refused for conflicting constraints'
ui_expect_log 'Sketch tool ended: Symmetry' 'Esc ends the Symmetry tool'
ui_expect_log 'Sketch conflict painted: none' 'leaving the tool paints nothing'
ui_expect_log 'TestDriver: done' 'every way of leaving puts the overlay back to the committed sketch'
ui_finish 'UI sketch conflict cleared on leaving a tool'
