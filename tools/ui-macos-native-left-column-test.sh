#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeMacWindow.mm app/platform/macos/NativeColumnSplit.mm app/MainWindowNative.cpp app/framework/ColumnShare.hpp
# The AppKit left column (mitcad#1020) on a synthetic design (mitcad-cli):
# three range options SizeX, SizeY and HeightUnits as configuration
# options, and 40 boxes, so that the Browser's rows are longer than the
# column.
#   - Configuration is a card of its own above Browser, one row per option:
#     the Row row and the three options under 190 points; its controls whole
#     and in view. Browser reaches the bottom of the free column.
#   - Dragging the boundary between them makes Configuration 80 points high
#     and Browser the rest; the split is remembered and restored after a
#     relaunch; a double-click returns to the automatic split.
#   - The disclosure folds Configuration to its header, remembered across a
#     relaunch (configuration/panelCollapsed, shared with Qt).
#   - With the Projects card shown (an empty projects folder), its boundary
#     with Configuration dragged down past the Projects rows moves no card
#     (mitcad#1152); the same after a relaunch.
# Usage: tools/ui-macos-native-left-column-test.sh <app> <mitcad-cli> <output>
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ $# -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output>" >&2
  exit 2
fi
ui_init "$1" "$3"
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
python3 - "$MITCAD_TEST_WORK/model.json" <<'PY'
import json
import sys

rect = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"
commands = [
    {"cmd": "add_parameter", "name": "SizeX", "unit": "mm", "expression": "84 mm",
     "min": "42 mm", "max": "252 mm", "step": "42 mm"},
    {"cmd": "add_parameter", "name": "SizeY", "unit": "mm", "expression": "84 mm",
     "min": "42 mm", "max": "252 mm", "step": "42 mm"},
    {"cmd": "add_parameter", "name": "HeightUnits", "unit": "mm", "expression": "21 mm",
     "min": "7 mm", "max": "70 mm", "step": "7 mm"},
    {"cmd": "sketch.create"},
    {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 84, "height": 84},
    {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": rect}],
                                   "extent": {"type": "distance", "distance": "HeightUnits"},
                                   "operation": "new_body"}},
]
for index in range(40):
    commands.append({"cmd": "add_feature", "def": {
        "type": "box", "plane": "xy", "corner": [100 + 10 * index, 0],
        "length": 5, "width": 5, "height": 5, "operation": "new_body"}})
commands.append({"cmd": "set_configurations", "configurations": {
    "options": ["SizeX", "SizeY", "HeightUnits"], "closed": False, "rows": [], "rules": []}})
with open(sys.argv[1], "w") as output:
    json.dump(commands, output)
PY
"$2" run "$MITCAD_TEST_WORK/model.json" --save "$MITCAD_TEST_WORK/column.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
run_phase() {
  MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-left-column-$1.mitcad-ui \
    ui_run_app "$1" --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/column.mitcad" || true
  ui_expect_log 'TestDriver: done' "$1: the native left column workflow reached its end"
  for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints' 'Native Mac canvas:' \
                 'OCCT view event failed'; do
    ui_expect_no_log "$problem" "$1: no $problem"
  done
  if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi
}
run_phase split
run_phase restored
run_phase collapsed
# The Projects panel shown from here on, as for a user who turned it on
# (mitcad#1020: hidden until then), with the run's empty projects folder.
# On macOS the private settings are nocodo.fi/Mitcad.ini (the organization
# domain, as tools/ui-macos-test.sh notes).
mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
printf '\n[projects]\npanel=true\n' >> "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
run_phase projects
run_phase projects-restored
echo "PASS: native Configuration card, shared left column, dragged and remembered split, disclosure, Projects boundary"
