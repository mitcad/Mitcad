#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeMacWindow.mm app/MainWindowNative.cpp app/browser/BrowserPanel.cpp
# The AppKit Browser's rows on a synthetic design (mitcad-cli): Plate:1
# (O1, Body1 of Extrude1 F2, grounded) and Pin:1 (O2) held by Joint1 (F5),
# and in the root Sketch3 (F6) used by Extrude3 (F7) and the suppressed
# Spare (F8), Sketch4 (F9) used by Missed cut (F10), a cut that misses
# every body, and Box1's body (F11.b0), shown when Holes == 'Yes'.
#   - The pointer over a body's, an occurrence's and the joint's row
#     highlights them in the view, as over the Qt rows (mitcad#1072);
#     leaving the rows ends it, and the selection stays empty.
#   - Missed cut's row is red, Spare's and the hidden body's grey
#     (mitcad#1079); the body's light bulb has the fx mark and its
#     condition's tooltip, the Bodies folder's the plain bulb.
# Usage: tools/ui-macos-native-browser-rows-test.sh <app> <mitcad-cli> <output>
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
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
sed "s/RECT/$RECT/g" > "$MITCAD_TEST_WORK/model.json" << 'JSON'
[
  {"cmd": "create_component", "name": "Plate"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Pin", "transform": {"translation": [100, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "ground_occurrence", "occurrence": "Plate:1"},
  {"cmd": "add_joint", "kind": "revolute", "flip": true,
    "a": {"occurrence": "Pin:1", "geometry": {"body": "F4.b0", "face": "F4:start(RECT)"}},
    "b": {"occurrence": "Plate:1", "geometry": {"body": "F2.b0", "face": "F2:end(RECT)"}}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [-100, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F6", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "name": "Spare", "def": {"type": "extrude", "profiles": [{"sketch": "F6", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "suppress_feature", "uid": "F8", "suppressed": true},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F9", "center": [900, 900], "diameter": 10},
  {"cmd": "add_feature", "name": "Missed cut", "def": {"type": "extrude", "profiles": [{"sketch": "F9", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "cut"}},
  {"expect": {"feature": "Missed cut", "status": "error"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-100, -60],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_parameter", "name": "Holes", "expression": "'No'"},
  {"cmd": "set_body_visible", "uid": "F11.b0", "expression": "Holes == 'Yes'"}
]
JSON
"$2" run "$MITCAD_TEST_WORK/model.json" --save "$MITCAD_TEST_WORK/rows.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-browser-rows.mitcad-ui \
  ui_run_app rows --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/rows.mitcad"
ui_expect_log 'TestDriver: done' 'native Browser rows workflow reached its end'
ui_expect_no_log 'TestDriver: FAIL|Native Mac canvas:|OCCT view event failed' 'native Browser rows retain input and rendering'
if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi
echo "PASS: native Browser row hover, colours and condition bulbs"
