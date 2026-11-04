#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp app/commands/SketchConstraintKeys.hpp app/sketch
# check-all sources: app/framework/CommandRegistry.cpp app/framework/TestDriver.cpp core/sketch tools/cli
# The sketch constraints' Shift keys (mitcad#1229), the shared input script
# tools/ui-scripts/sketch-constraint-shortcuts.mitcad-ui (also run by
# tools/ui-windows-sketch-constraint-shortcuts-test.ps1): two lines
# selected, Shift+P adds Parallel; a line selected, Shift+V makes it
# vertical; Shift+P typed into a dimension's value field types a capital P
# and adds no constraint.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-constraint-shortcuts-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-constraint-shortcuts.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-constraint-shortcuts.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-constraint-shortcuts.json" --save "$WORK/constraints.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/constraints.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch constraint keys driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Shift+P adds Parallel, Shift+V Vertical, Shift+P in a value field types P'
ui_finish 'UI sketch constraint Shift keys'
