#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/gravity app/MainWindowGravity.cpp app/commands/GravityCommands.cpp app/framework/CommandPanel.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowContextMenu.cpp app/framework/CommandSession.cpp
# check-all sources: core/model-api/src/gravity.rs core/model/src/document/gravity.rs core/model/src/document/flex_hang.rs
# check-all sources: tools/ui-scripts/gravity.mitcad-ui tools/ui-scripts/gravity.json tools/cli
# Let It Hang and its panel's direction of gravity (mitcad#1713) on macOS,
# through the test driver (tools/ui-scripts/gravity.mitcad-ui, the same
# script as tools/ui-gravity-test.sh and tools/ui-windows-gravity-test.ps1)
# on the design that mitcad-cli makes (tools/ui-scripts/gravity.json: a
# plate, a step on it, a plank above the step's edge and a hose over them,
# no direction of gravity), the Qt frontend in its default layout: the
# panel asks for a direction, the hose hangs onto the plank, is pinned,
# dragged and kept with the direction as one undo step; Esc and Clear
# Gravity leave the design as it was.
# Usage: tools/ui-macos-gravity-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-gravity}"
UI_TIMEOUT=360
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/gravity.json" --save "$UI_OUT/files/gravity.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=100 MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_INPUT=$here/ui-scripts/gravity.mitcad-ui \
  ui_run_app gravity --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/gravity.mitcad"
ui_expect_log '^Hang panel opened' 'Let It Hang opened its panel from the hose'"'"'s context menu'
ui_expect_log '^Hang started F4 \(Hose 6\): ' 'the hose let hang once the direction was given'
ui_expect_log '^Hang locked F4: nodes n1,' 'OK kept its shape'
ui_expect_log '^Let It Hang: its steps merged into Edit Flex1' 'the direction and the shape are one undo step'
ui_expect_log '^Let It Hang: 1 direction change\(s\) taken back' 'Esc took the direction back'
ui_expect_log '^Gravity cleared' 'Clear Gravity in the panel cleared the direction'
# Where the hose rested before it was kept: lying on the plank (three of
# its points or more on a body) and nowhere through the plate (its lowest
# point at least the plate's top, 10, and the hose's radius, 3, high).
rest=$(grep -a -B1000 '^Hang locked F4: ' "$UI_LOG_FILE" | grep -a '^Hang rest F4: ' | tail -n 1)
lowest=$(sed -nE 's/.*, lowest ([-0-9.]+) along .*/\1/p' <<< "$rest")
touching=$(sed -nE 's/.*, touching ([0-9]+), .*/\1/p' <<< "$rest")
if [ -n "$lowest" ] && [ -n "$touching" ] && ! ui_lt "$lowest" 12.9 && ! ui_lt "$touching" 3; then
  ui_pass "the hose lies on the plank, $touching points on it, its lowest point at $lowest"
else
  ui_fail "the hose does not lie on the plank: ${rest:-no resting hose logged}"
fi
ui_expect_log 'TestDriver: done' 'the script ran to its end'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
