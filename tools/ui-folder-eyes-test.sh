#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/MainWindowVisibility.cpp app/files/MainWindowLocks.cpp
# check-all sources: core/model/src/document/display.rs core/model/src/document/components.rs
# check-all sources: core/model-api/src/lib.rs tools/cli
# The eyes of the Browser's Bodies and Sketches folders (mitcad#1771,
# GitHub mitcad/Mitcad#292) through the real UI, on a design mitcad-cli
# makes: five boxes (F1-F5, their bodies F1.b0-F5.b0) and five sketches
# (F6-F10) on XY, Body2, Body4, Sketch2 and Sketch4 hidden by their own
# eyes.
#   - The Sketches folder's eye hides every sketch, one undo step `Hide
#     Sketches`; the rows are greyed with their own eyes as they are. A
#     sketch's eye still changes while the folder is hidden. Shown again,
#     the folder shows the same three sketches as before.
#   - The same for the Bodies folder (`Hide Bodies`).
#   - Both folders hidden, the design saved: the file keeps the folders'
#     eyes as display state and the items' own eyes as they were. Opened
#     again, both folders are hidden; their eyes show them, and the view
#     shows the same three sketches and bodies as before.
#   - Hide All in the Sketches folder's menu still turns off every sketch's
#     own eye.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-folder-eyes-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-folder-eyes.XXXXXX")
FILE=$WORK/folders.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# expect_file filter expected description: a jq filter on the saved file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# expect_none "text" description: no line since the mark has the text.
expect_none() {
  ui_sync
  tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2: '$1' logged"
  echo "ok   $2"
}

# The sketches and the bodies hidden by their own eyes in the saved file.
HIDDEN_SKETCHES='[.features[] | select(.type == "sketch" and .visible == false) | .uid]'
HIDDEN_BODIES='[.bodies[] | select(.visible == false) | .uid]'

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": 20,
    "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [30, 0], "length": 20,
    "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [60, 0], "length": 20,
    "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [90, 0], "length": 20,
    "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [120, 0], "length": 20,
    "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": "xy"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [0, 30], "width": 20, "height": 10},
  {"cmd": "sketch.create", "plane": "xy"},
  {"cmd": "sketch.add_rectangle", "sketch": "F7", "corner": [30, 30], "width": 20, "height": 10},
  {"cmd": "sketch.create", "plane": "xy"},
  {"cmd": "sketch.add_rectangle", "sketch": "F8", "corner": [60, 30], "width": 20, "height": 10},
  {"cmd": "sketch.create", "plane": "xy"},
  {"cmd": "sketch.add_rectangle", "sketch": "F9", "corner": [90, 30], "width": 20, "height": 10},
  {"cmd": "sketch.create", "plane": "xy"},
  {"cmd": "sketch.add_rectangle", "sketch": "F10", "corner": [120, 30], "width": 20, "height": 10},
  {"cmd": "set_feature_visible", "uid": "F7", "visible": false},
  {"cmd": "set_feature_visible", "uid": "F9", "visible": false},
  {"cmd": "set_body_visible", "uid": "F2.b0", "visible": false},
  {"cmd": "set_body_visible", "uid": "F4.b0", "visible": false}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
expect_file "$HIDDEN_SKETCHES" '["F7","F9"]' "the design: two of five sketches hidden"
expect_file "$HIDDEN_BODIES" '["F2.b0","F4.b0"]' "the design: two of five bodies hidden"
expect_file '.display.folders' 'null' "the design: no folder hidden"

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Sketches shown: F6, F8, F10" "three sketches drawn"
ui_expect_log "Bodies shown: F1.b0, F3.b0, F5.b0" "three bodies drawn"
ui_expect_log "Browser eye Root/Sketches/Sketch2 at" "Sketch2's eye in the browser"

echo "--- The Sketches folder's eye"
ui_mark
ui_step "hide the Sketches folder"         ui_click_logged "Browser eye Root/Sketches"
ui_expect_new "Visibility Root/Sketches: hidden" "the folder hidden"
ui_expect_new "Sketches shown: none" "no sketch drawn"
ui_sync
colours=$(grep "^Browser row colours: " "$UI_LOG" | tail -1)
# The hidden group lists its first dozen paths, sorted, then "<n> more":
# with the hidden Origin's rows before them, Sketch3 may be among the rest.
hidden=${colours#*hidden }
hidden=${hidden%%;*}
[[ $hidden == *"Root/Sketches/Sketch1"* &&
  ($hidden == *"Root/Sketches/Sketch3"* || $hidden == *"Root/Sketches/Sketch1, "*" more") ]] ||
  ui_fail "the shown sketches' rows are not greyed: $colours"
echo "ok   the sketches' rows greyed"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Hide Sketches" "one undo step"
ui_expect_new "Sketches shown: F6, F8, F10" "the three sketches back"
ui_mark
ui_step "hide the Sketches folder again"   ui_click_logged "Browser eye Root/Sketches"
ui_expect_new "Visibility Root/Sketches: hidden" "the folder hidden again"
ui_expect_new "Sketches shown: none" "no sketch drawn"

echo "--- A sketch's eye while its folder is hidden"
ui_mark
ui_step "show Sketch2"                     ui_click_logged "Browser eye Root/Sketches/Sketch2"
ui_expect_new "Visibility Root/Sketches/Sketch2: shown" "Sketch2's eye on"
expect_none "Sketches shown: F" "nothing drawn while the folder is hidden"
ui_mark
ui_step "hide Sketch2 again"               ui_click_logged "Browser eye Root/Sketches/Sketch2"
ui_expect_new "Visibility Root/Sketches/Sketch2: hidden" "Sketch2's eye off again"

ui_mark
ui_step "show the Sketches folder"         ui_click_logged "Browser eye Root/Sketches"
ui_expect_new "Visibility Root/Sketches: shown" "the folder shown"
ui_expect_new "Sketches shown: F6, F8, F10" "the same three sketches drawn, Sketch2 and Sketch4 still hidden"

echo "--- The Bodies folder's eye"
ui_mark
ui_step "hide the Bodies folder"           ui_click_logged "Browser eye Root/Bodies"
ui_expect_new "Visibility Root/Bodies: hidden" "the folder hidden"
ui_expect_new "Bodies shown: none" "no body drawn"
ui_mark
ui_step "show the Bodies folder"           ui_click_logged "Browser eye Root/Bodies"
ui_expect_new "Visibility Root/Bodies: shown" "the folder shown"
ui_expect_new "Bodies shown: F1.b0, F3.b0, F5.b0" "the same three bodies drawn"

echo "--- Both hidden, saved and opened again"
ui_mark
ui_step "hide the Sketches folder"         ui_click_logged "Browser eye Root/Sketches"
ui_expect_new "Sketches shown: none" "no sketch drawn"
ui_step "hide the Bodies folder"           ui_click_logged "Browser eye Root/Bodies"
ui_expect_new "Bodies shown: none" "no body drawn"
ui_focus_main
ui_save_if_changed "$FILE"
expect_file '.display.folders' '{"C0":{"bodies":true,"sketches":true}}' "the file keeps both folders hidden"
expect_file "$HIDDEN_SKETCHES" '["F7","F9"]' "the sketches' own eyes as they were"
expect_file "$HIDDEN_BODIES" '["F2.b0","F4.b0"]' "the bodies' own eyes as they were"

ui_stop_app
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Browser eye Root/Sketches at" "the design opened again"
ui_sync
grep -q "^Sketches shown: F" "$UI_LOG" && ui_fail "a sketch is drawn while its folder is hidden"
grep -q "^Bodies shown: F" "$UI_LOG" && ui_fail "a body is drawn while its folder is hidden"
echo "ok   nothing drawn: both folders opened hidden"
ui_mark
ui_step "show the Sketches folder"         ui_click_logged "Browser eye Root/Sketches"
ui_expect_new "Visibility Root/Sketches: shown" "its eye was off: it shows the folder"
ui_expect_new "Sketches shown: F6, F8, F10" "the same two sketches hidden"
ui_step "show the Bodies folder"           ui_click_logged "Browser eye Root/Bodies"
ui_expect_new "Visibility Root/Bodies: shown" "its eye was off: it shows the folder"
ui_expect_new "Bodies shown: F1.b0, F3.b0, F5.b0" "the same two bodies hidden"
ui_focus_main
ui_save_if_changed "$FILE"
expect_file '.display.folders' 'null' "no folder hidden in the file"
expect_file "$HIDDEN_SKETCHES" '["F7","F9"]' "the same two sketches hidden in the file"
expect_file "$HIDDEN_BODIES" '["F2.b0","F4.b0"]' "the same two bodies hidden in the file"

echo "--- Hide All in the folder's menu"
ui_mark
ui_step "right-click Sketches"             ui_click_logged "Browser Root/Sketches" 3
ui_expect_new "Context menu: " "the folder's menu"
ui_step "choose Hide All"                  ui_menu_choose "Hide All"
ui_expect_new "Visibility Root/Sketches: hidden" "Hide All logged"
ui_expect_new "Sketches shown: none" "no sketch drawn"
ui_focus_main
ui_save_if_changed "$FILE"
expect_file "$HIDDEN_SKETCHES" '["F6","F7","F8","F9","F10"]' "Hide All turned off every sketch's own eye"
expect_file '.display.folders' 'null' "and left the folder's eye on"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI folder eyes test"
