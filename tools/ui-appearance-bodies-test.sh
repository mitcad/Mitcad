#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/Appearances.cpp app/framework/Appearances.hpp app/MainWindow.cpp tools/cli app/MainWindowContextMenu.cpp app/commands/MaterialCommands.cpp
# Appearance… of selected bodies and faces (mitcad#346; the Appearance tool
# since mitcad#1059) through the real UI, on three boxes side by side
# (F1.b0, F2.b0, F3.b0):
#   - Body1's row: Appearance… starts the Appearance tool with Body1 in its
#     Bodies; no window opens.
#   - Two faces of Body1 and a face of Body2 selected: Appearance… starts
#     the tool with the three faces in its Faces; Red Copy chosen in the
#     picker and OK give the three faces it in one undo step, the bodies
#     keep the default look, Body3 untouched.
#   - Undo takes the faces' appearances back at once.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-appearance-bodies-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-appearance-bodies.XXXXXX)
FILE=$WORK/boxes.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# expect_file filter expected description: a jq filter on the saved project
# file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

save() {
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

# ctrl_click x y: a click with Ctrl held, adding to the selection.
ctrl_click() { xdotool mousemove "$1" "$2" keydown ctrl click 1 keyup ctrl; }

# Three 40 x 20 x 20 boxes 10 mm apart along X, and a copy of Paint - Red
# in the design.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-75, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-20, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [35, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_appearance", "id": "red_copy", "name": "Red Copy", "based_on": "paint_red"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Body F1.b0 at" "the first box is in the view"
ui_expect_log "Body F3.b0 at" "and the third"

echo "--- A body's row: the Appearance tool for it"
ui_mark
ui_step "right-click Body1's row"        ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "choose Appearance..."           ui_menu_choose "Appearance..."
ui_expect_new "Appearance tool for F1.b0" "the Appearance tool for the body"
ui_expect_new "Appearance Bodies: 1 body [body F1.b0]" "Body1 in its Bodies"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Appearances dialog opened"; then
  ui_fail "Appearance… opened the Appearances window"
fi
echo "ok   no window opened"
ui_step "Cancel (Escape)"                ui_key Escape
ui_step "Escape (clear the selection)"   ui_key Escape

echo "--- Faces of two bodies selected"
ui_mark
read -r bx by <<< "$(ui_logged_at "Body F1.b0")"
read -r cx cy <<< "$(ui_logged_at "Body F2.b0")"
# The logged place is where the body's corner nearest the camera is: its top
# face is above it, a side face below.
ui_step "click the top face of Body1"    xdotool mousemove "$bx" $((by - 20)) click 1
ui_expect_new "Selected: 1 face [face" "a face of Body1 selected"
ui_step "Ctrl+click a side face of Body1" ctrl_click $((bx + 12)) $((by + 40))
ui_step "Ctrl+click the top face of Body2" ctrl_click "$cx" $((cy - 20))
ui_expect_new "Selected: 3 faces" "three faces selected"
ui_mark
ui_step "right-click Body2's selected face"    xdotool mousemove "$cx" $((cy - 20)) click 3
ui_step "choose Appearance..."           ui_menu_choose "Appearance..."
ui_expect_new "Appearance tool for " "the Appearance tool for the faces"
ui_expect_new "Appearance Faces: 3 faces" "the three faces in its Faces"
ui_step "the picker: Red Copy"           ui_picker_choose "Panel Appearance input value" "Appearance picker" "Red Copy"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Appearance red_copy for 0 body(ies), 3 face(s)" "the three faces got it"
save
expect_file '[.bodies[] | .appearance]' '[null,null,null]' "the bodies keep the default look"
expect_file '[.bodies[] | has("face_appearances")]' '[true,true,false]' \
  "Body1's two faces and Body2's face have it as their own"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Appearance" "one undo step"
save
expect_file '[.bodies[] | has("face_appearances")]' '[false,false,false]' "no face appearance of its own again"

ui_finish "UI Appearance of bodies and faces"
