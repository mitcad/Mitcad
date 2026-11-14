#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/BrowserController.cpp app/browser/DocumentSnapshot.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowVisibility.cpp core/model/src/document/display.rs tools/cli
# The browser's eyes while Isolate is on (mitcad#1700) through the real UI,
# on three components side by side (Component1:1, Component2:1 and
# Component3:1, occurrences O1, O2, O3, bodies F2.b0, F4.b0, F6.b0):
#   - Isolate Component1:1: the other two rows' eyes are off ("isolated
#     out"), the view shows Component1's body only.
#   - A click on Component2:1's eye shows it too (its eye on, one undo step
#     that sets no light bulb); another click hides it again.
#   - Undo takes those steps back one by one, Redo brings one back.
#   - Unisolate shows all three again with their eyes on.
#   - Component3:1 hidden by its own light bulb, then Component1:1
#     isolated: a click on Component3:1's eye sets its light bulb on and
#     adds it to the isolation, one undo step that takes back both.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-isolate-eyes-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-isolate-eyes.XXXXXX")
FILE=$WORK/three.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# Three 20 x 20 squares 40 mm apart, each extruded 10 up as a new
# component: Sketch1 (F1) and Extrude1 (F2, Component1), Sketch2 (F3) and
# Extrude2 (F4, Component2), Sketch3 (F5) and Extrude3 (F6, Component3).
cat > "$WORK/three.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_component"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [40, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_component"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [80, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_component"}}
]
EOF
"$CLI" run "$WORK/three.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# since_mark "text": whether a line logged after the mark has the text.
since_mark() { tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qF -- "$1"; }

# expect_none "text" description: no line since the mark has the text.
expect_none() {
  ui_sync
  since_mark "$1" && ui_fail "$2: '$1' logged"
  echo "ok   $2"
}

# expect_shown "O1 O2" description: the view's last "Bodies shown" line
# lists the bodies of exactly these occurrences.
expect_shown() {
  ui_sync
  local line wanted occurrence
  line=$(grep -F "Bodies shown: " "$UI_LOG" | tail -1)
  for occurrence in O1 O2 O3; do
    wanted=0
    case " $1 " in *" $occurrence "*) wanted=1 ;; esac
    if grep -qF " in $occurrence" <<< "$line"; then
      [ "$wanted" = 1 ] || ui_fail "$2: $occurrence is shown ($line)"
    else
      [ "$wanted" = 0 ] || ui_fail "$2: $occurrence is not shown ($line)"
    fi
  done
  echo "ok   $2"
}

# isolate_first: Isolate from Component1:1's row.
isolate_first() {
  ui_mark
  ui_step "right-click Component1:1"     ui_click_logged "Browser Root/Component1:1" 3
  ui_step "choose Isolate"               ui_menu_choose "Isolate"
  ui_expect_new "Isolated " "Component1:1 isolated"
}

# unisolate: Unisolate from Component1:1's row.
unisolate() {
  ui_mark
  ui_step "right-click Component1:1"     ui_click_logged "Browser Root/Component1:1" 3
  ui_step "choose Unisolate"             ui_menu_choose "Unisolate"
  ui_expect_new "Unisolated" "Unisolate"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: " "the bodies shown"
expect_shown "O1 O2 O3" "all three components shown"

echo "--- Isolate Component1:1: the others' eyes off"
isolate_first
ui_expect_new "Browser eye Root/Component2:1: isolated out" "Component2:1's eye off"
ui_expect_new "Browser eye Root/Component3:1: isolated out" "Component3:1's eye off"
expect_none "Browser eye Root/Component1:1: isolated out" "Component1:1's eye stays on"
expect_shown "O1" "only Component1:1 shown"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- Component2:1's eye shows it too, another click hides it"
ui_mark
ui_step "Component2:1's eye"             ui_click_logged "Browser eye Root/Component2:1"
ui_expect_new "Isolation: added Root/Component2:1" "Component2:1 added to the isolation"
ui_expect_new "Browser eye Root/Component2:1: isolated in" "its eye on"
expect_none "Visibility Root/Component2:1" "its light bulb not set"
expect_none "Browser eye Root/Component3:1: isolated in" "Component3:1 stays out"
expect_shown "O1 O2" "Component1:1 and Component2:1 shown"
ui_mark
ui_step "Component2:1's eye again"       ui_click_logged "Browser eye Root/Component2:1"
ui_expect_new "Isolation: removed Root/Component2:1" "Component2:1 removed from the isolation"
ui_expect_new "Browser eye Root/Component2:1: isolated out" "its eye off again"
expect_none "Visibility Root/Component2:1" "its light bulb not set"
expect_shown "O1" "only Component1:1 shown again"

echo "--- Undo and Redo step through the isolation"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Isolate an item" "one undo step takes the removal back"
ui_expect_new "Browser eye Root/Component2:1: isolated in" "Component2:1's eye on again"
expect_shown "O1 O2" "Component2:1 shown again"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Isolate 2 items" "the next takes the addition back"
ui_expect_new "Browser eye Root/Component2:1: isolated out" "Component2:1's eye off"
expect_shown "O1" "only Component1:1 shown"
ui_mark
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_expect_new "Redo: Isolate 2 items" "Redo adds Component2:1 again"
ui_expect_new "Browser eye Root/Component2:1: isolated in" "its eye on"
expect_shown "O1 O2" "Component1:1 and Component2:1 shown"

echo "--- Unisolate: all three shown with their eyes on"
unisolate
ui_expect_new "Browser eye Root/Component3:1: isolated in" "Component3:1's eye on"
expect_shown "O1 O2 O3" "all three shown again"
expect_none "Browser eye Root/Component1:1: isolated out" "Component1:1's eye stayed on"

echo "--- Component3:1 hidden by its light bulb, outside the isolation: its eye shows and adds it"
ui_mark
ui_step "hide Component3:1"              ui_click_logged "Browser eye Root/Component3:1"
ui_expect_new "Visibility Root/Component3:1: hidden" "Component3:1 hidden by its light bulb"
expect_shown "O1 O2" "Component3:1 hidden"
isolate_first
expect_shown "O1" "only Component1:1 shown"
ui_mark
ui_step "Component3:1's eye"             ui_click_logged "Browser eye Root/Component3:1"
ui_expect_new "Visibility Root/Component3:1: shown" "its light bulb on"
ui_expect_new "Isolation: added Root/Component3:1" "and it joins the isolation"
ui_expect_new "Browser eye Root/Component3:1: isolated in" "its eye on"
expect_shown "O1 O3" "Component1:1 and Component3:1 shown"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Show Component3:1" "one undo step for both"
ui_expect_new "Browser eye Root/Component3:1: isolated out" "Component3:1 out of the isolation again"
expect_shown "O1" "only Component1:1 shown"
unisolate
expect_shown "O1 O2" "Component3:1 still hidden by its own light bulb"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI Isolate eyes test"
