#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeForms.mm app/framework/ResultRows.cpp app/commands/InspectCommands.cpp
# check-all sources: app/files/MainWindowPrint.cpp app/platform/macos/NativeTestInput.mm app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/native-macos-panel-rows.mitcad-ui tools/cli
# The AppKit command form's check box with its text and an inspection's
# rows (mitcad#1354), as the shared panel shows them: Send to Slicer's Use
# the open slicer window is one check box with its text across the form,
# which leaves the captions' column as wide as its widest caption; Measure
# between two sketch points shows Distance, Δx, Δy and Δz as rows and a
# group for each pick that folds
# (tools/ui-scripts/native-macos-panel-rows.mitcad-ui). In-process native
# input only: no system events.
# Usage: tools/ui-macos-native-panel-rows-test.sh <app> <mitcad-cli> <output directory>
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
# A box away from the origin (F1.b0) for Send to Slicer, and a sketch (F2)
# with two free points, p1 at (10, 10) and p2 at (40, 50), for Measure.
cat > "$MITCAD_TEST_WORK/panel-rows.json" << 'JSON'
[
  {"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[-80,-80],"length":20,"width":20,"height":10,
    "operation":"new_body"}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_point","sketch":"F2","at":[10,10]},
  {"cmd":"sketch.add_point","sketch":"F2","at":[40,50]}
]
JSON
"$cli" run "$MITCAD_TEST_WORK/panel-rows.json" --save "$MITCAD_TEST_WORK/panel-rows.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }

MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-panel-rows.mitcad-ui \
  ui_run_app native-panel-rows --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/panel-rows.mitcad"
ui_expect_log '^Panel Send to Slicer input reuse: a check box with its text across the form' \
  'Use the open slicer window is one check box with its text'
# The captions' column as wide as its widest caption, narrower than the
# check box's text, which is not in it (the last layout logged).
column_line=$(grep -E '^Panel Send to Slicer captions column: ' "$UI_LOG_FILE" | tail -n 1)
check_line=$(grep -E '^Panel Send to Slicer input reuse: a check box with its text' "$UI_LOG_FILE" | tail -n 1)
column=$(sed -n 's/.*captions column: \([0-9]*\) wide.*/\1/p' <<< "$column_line")
caption=$(sed -n 's/.*its widest caption [^ ]* \([0-9]*\) wide.*/\1/p' <<< "$column_line")
check=$(sed -n 's/.*across the form, \([0-9]*\) wide.*/\1/p' <<< "$check_line")
if [ -n "$column" ] && [ -n "$caption" ] && [ -n "$check" ] && [ "$column" -le $((caption + 1)) ] &&
   [ "$column" -lt "$check" ]; then
  ui_pass "the captions' column is ${column} wide, as its widest caption (${caption}), not the check box's text (${check})"
else
  ui_fail "the captions' column is not its widest caption's: '$column_line', '$check_line'"
fi
ui_expect_log '^Native command layout: ' 'the form keeps its captions and controls in aligned columns'
ui_expect_log '^Panel Measure result values: Distance = 50 mm, ' 'Measure shows Distance, then the deltas, as rows'
ui_expect_log '^Panel Measure result group 1 folded$' "a pick's group folds by its header"
ui_expect_log '^TestDriver: done' 'the check box text and the result rows are shown as in the Qt frontend'
for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints'; do
  ui_expect_no_log "$problem" "no $problem"
done
if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
echo 'PASS: AppKit check box text and inspection rows'
