#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/FormulaEditor.cpp app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-equation-curve.mitcad-ui tools/ui-scripts/sketch-equation-curve.json
# Equation Curve (mitcad#2237), the shared input script
# tools/ui-scripts/sketch-equation-curve.mitcad-ui (also run by
# tools/ui-windows-sketch-equation-curve-test.ps1 and
# tools/ui-macos-sketch-equation-curve-test.sh): an involute of a parameter
# made in the panel, its closed profile extruded, the parameter changed and
# the curve, the profile and the body following, then its formula edited.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-equation-curve-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-equation-curve.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-equation-curve.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-equation-curve.json" --save "$WORK/equation-curve.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/equation-curve.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch equation curve driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'an involute of a parameter made, extruded, followed and edited'
ui_finish 'UI sketch Equation Curve involute follows its parameter'
