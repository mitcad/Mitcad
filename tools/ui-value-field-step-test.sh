#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ParameterField.cpp app/framework/ValueStep.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp app/commands/PatternCommands.cpp
# check-all sources: tools/ui-scripts/value-field-step.mitcad-ui tools/ui-scripts/value-field-step.json tools/cli
# check-all sources: app/sketch/SketchController.cpp app/commands/PrimitiveCommands.cpp
# Up, Down and the mouse wheel step a value field's number (mitcad#1254,
# GitHub mitcad/Mitcad#129), and so do the step buttons at its right end
# and the 3D view's value boxes (mitcad#2327, GitHub mitcad/Mitcad#594),
# through the real UI, the shared input script
# tools/ui-scripts/value-field-step.mitcad-ui (also run by
# tools/ui-windows-value-field-step-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/value-field-step.json): Extrude's Distance
# holding d1*2 is not stepped, by Up nor by its disabled up button, 10 mm
# and Shift+Up is 20 mm, the up button 21 mm, Shift and the up button
# 31 mm, the down button held steps it down again and again; Box's Height
# in its value box in the view steps with Up, Shift+Up and the wheel, the
# view neither turned nor zoomed; Circular Pattern's Quantity 4 and Up is
# 5, previewed; the wheel over a field without the keyboard changes
# nothing, over the one with it steps it; a sketch rectangle's Width 40 mm
# and its up button is 41 mm.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-value-field-step-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-value-field-step.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/value-field-step.mitcad-ui
"$cli" run "$here/ui-scripts/value-field-step.json" --save "$work/value-field-step.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/value-field-step.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'value field step driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Value field Extrude Distance: Up not stepped: d1*2 is not a plain value' 'an expression is not stepped'
ui_expect_log 'Value field Extrude Distance: Shift+Up stepped 10 mm to 20 mm' 'Shift+Up steps a distance by 10 mm'
ui_expect_log 'Value field Circular Pattern Quantity: Up stepped 4 to 5' 'Up steps the quantity from 4 to 5'
ui_expect_log 'Value field Circular Pattern Quantity: wheel stepped 5 to 6' 'the wheel steps the field with the keyboard'
ui_expect_log 'Value field Extrude Distance: Up button stepped 20 mm to 21 mm' 'the up button steps a distance by 1 mm'
ui_expect_log 'Value field Extrude Distance: Shift+Up button stepped 21 mm to 31 mm' 'Shift and the up button step by 10 mm'
ui_expect_log 'Value field Extrude Distance: Down button held stepped 30 mm to 29 mm' 'the down button held repeats'
ui_expect_log 'Value field Box Height: Shift+Up stepped 11 mm to 21 mm' "Shift+Up steps the view's value box"
ui_expect_log 'Value field Box Height: wheel stepped 21 mm to 22 mm' "the wheel steps the view's value box"
ui_expect_log 'Value field sketch width: Up button stepped 40 mm to 41 mm' "a sketch tool field's up button steps it"
ui_expect_log 'TestDriver: done' 'Up, Down, the wheel and the step buttons step the values'
ui_finish 'UI value fields step with Up, Down, the wheel and the step buttons'
