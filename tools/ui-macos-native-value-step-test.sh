#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeForms.mm app/platform/macos/NativeInlineEditor.mm app/platform/macos/NativeKeys.hpp
# check-all sources: app/framework/ParameterField.cpp app/framework/ValueStep.cpp app/framework/TestDriver.cpp
# check-all sources: app/platform/macos/NativeTestInput.mm tools/ui-scripts/native-macos-value-step.mitcad-ui
# Stepping a value in the AppKit frontend (mitcad#1350), as
# tools/ui-value-field-step-test.sh checks the Qt frontend's (mitcad#1254):
# the rectangle tool's Width field steps 40 mm to 41 mm with Up and the
# rectangle is added 41 mm wide; on a second rectangle without dimensions
# the Dimension tool's editor steps 20.00 to 21 with Up and the dimension
# is added with 21; Extrude's Distance
# holding an expression is not stepped, 10 mm steps with Up, Shift+Up,
# Option+Down and the wheel to 21.9 mm, which the extrusion takes; both
# kinds of field say so in their tooltips
# (tools/ui-scripts/native-macos-value-step.mitcad-ui). The keys are the
# in-process native keys, the wheel the in-process native wheel: no system
# input.
# Usage: tools/ui-macos-native-value-step-test.sh <app> <output directory>
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 2 ]; then
  echo "usage: $0 <app> <output directory>" >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"

MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-value-step.mitcad-ui \
  ui_run_app native-value-step --ui=native --no-recovery --no-mcp
ui_expect_log '^Value field sketch width: Up stepped 40 mm to 41 mm$' "Up steps the rectangle tool's Width"
ui_expect_log '^Value field sketch dimension: Up stepped 20[.]00 to 21$' "Up steps the dimension editor's value"
ui_expect_log '^Value field Extrude Distance: Up not stepped: 5 mm [*] 2 is not a plain value$' 'an expression is not stepped'
ui_expect_log '^Value field Extrude Distance: Alt[+]Down stepped 21 mm to 20[.]9 mm$' 'Option+Down steps by 0.1 mm'
ui_expect_log '^Value field Extrude Distance: wheel stepped 20[.]9 mm to 21[.]9 mm$' 'the wheel steps the field with the keyboard'
ui_expect_log '^TestDriver: done' 'Up, Down and the wheel step the AppKit value fields'
for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints'; do
  ui_expect_no_log "$problem" "no $problem"
done
if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
echo 'PASS: AppKit value fields step with Up, Down and the wheel'
